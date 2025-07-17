use std::process::{Command, Stdio};

use color_eyre::eyre::{eyre, Context, Result};
use regex::{Regex};

#[derive(Default)]
pub struct FoundPod {
    pub name: String,
    pub namespace: String,
    pub deployment: String
}

pub fn find_matching_deployment(matcher: &str, namespace: &str) -> Result<String> {

    let deployment_output = {
        Command::new("kubectl")
            .arg("get")
            .arg("deployments")
            .arg("-n")
            .arg(&namespace)
            .output()
            .wrap_err("Could not get deployments")
    }?;

    let deployments = String::from_utf8(deployment_output.stdout)?.to_string();

    // Strip numbers and dashes from the matcher
    let sanitised_matcher = Regex::new(r"\-+[0-9]+")?.replace_all(matcher, "");

    let re = Regex::new(&format!(r"[A-Za-z-]*{sanitised_matcher}[A-Za-z-]* "))?;

    let deployment_matches = re.captures(&deployments);
    
    match deployment_matches {
        Some(matches) => {
            let deployment: String = matches[0].to_string().replace(" ", "");
        
            Ok(deployment)
        },
        None => {
            Err(eyre!(format!("Failed to find deployment for given pod {} in namespace {}", &sanitised_matcher, &namespace)))
        },
    }
}


pub fn find_matching_pod(matcher: &str) -> Result<FoundPod> {
    let output = {
        Command::new("kubectl")
            .arg("get")
            .arg("pods")
            .arg("--all-namespaces")
            .output()
            .wrap_err("Could not execute kubectl get pods")
    }?;

    let pods = String::from_utf8(output.stdout).unwrap().to_string();

    let re = Regex::new(&format!(r"(\b.*\b)( .*{matcher}.*-[0-9A-Za-z-]+)")).unwrap();

    match re.captures(&*pods) {
        Some(matches) => {
            let pod: String = matches[2].replace(" ", "");
            let ns: String = matches[1].to_string();
            let deployment: String = find_matching_deployment(&matcher, &ns)?;

            let found_pod : FoundPod = FoundPod {
                name: pod,
                namespace: ns,
                deployment: deployment
            };

            Ok(found_pod)
        },
        None => Err(eyre!(format!("Failed to find pod for given matcher {}", &matcher))),
    }
}

pub fn debug_pod(pod: &FoundPod) -> anyhow::Result<()> {
    // Get image name.
    let image_name = String::from_utf8(Command::new("kubectl")
        .arg("get")
        .arg("pod")
        .arg(&pod.name)
        .arg("-n")
        .arg(&pod.namespace)
        .arg("-o=jsonpath={.spec.containers[0].image}")
        .output()
        .unwrap()
        .stdout).unwrap().replace("[ ", "");

    let container_name = String::from_utf8(Command::new("kubectl")
        .arg("get")
        .arg("pod")
        .arg(&pod.name)
        .arg("-n")
        .arg(&pod.namespace)
        .arg("-o=jsonpath={.spec.containers[0].name}")
        .output()
        .unwrap()
        .stdout).unwrap().replace("[ ", "");

    
    print!("{}", &image_name);

    let _output = {
        Command::new("kubectl")
            .arg("debug")
            .arg(&pod.name)
            .arg("-n")
            .arg(&pod.namespace)
            .arg("-it")
            .arg(format!("--image={}", &image_name))
            .arg(format!("--target={}", &container_name))
            .arg("--")
            .arg("sh")
            .spawn()
            .unwrap()
            .wait()
            .expect("failed to execute process")
    };

    Ok(())
}

pub fn exec_into_pod(pod: &FoundPod) -> anyhow::Result<()> {
    let _output = {
        Command::new("kubectl")
            .arg("exec")
            .arg("--stdin")
            .arg("--tty")
            .arg(&pod.name)
            .arg("-n")
            .arg(&pod.namespace)
            .arg("--")
            .arg("/bin/sh")
            .spawn()
            .unwrap()
            .wait()
            .expect("failed to execute process")
    };

    Ok(())
}

pub fn delete_pod(pod: &FoundPod) -> anyhow::Result<String> {
    let output = {
        Command::new("kubectl")
            .arg("delete")
            .arg("pod")
            .arg(&pod.name)
            .arg("-n")
            .arg(&pod.namespace)
            .arg("--wait=false")
            .output()
            .expect("failed to execute process")
    };

    let delete = String::from_utf8(output.stdout).unwrap().to_string();

    Ok(delete)
}

pub fn get_pods(pod: &FoundPod) -> anyhow::Result<String> {
    let output = {
        Command::new("kubectl")
            .arg("get")
            .arg("pods")
            .arg("-n")
            .arg(&pod.namespace)
            .arg("--sort-by=.status.startTime")
            .arg("--no-headers")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap()
    };

    let tac = {
        Command::new("tac")
            .stdin(Stdio::from(output.stdout.unwrap()))
            .output()
            .expect("failed to execute process")
    };

    let pods = String::from_utf8(tac.stdout).unwrap().
        replace("Running", "🏃 Running").
        replace("Error", "❌ Error").
        replace("Completed", "✅ Completed").
        replace("Terminating", "💀️ Terminating").
        replace("CrashLoopBackOff", "🔥 CrashLoopBackOff").
        replace("ImagePullBackOff", "👻 ImagePullBackOff").
        replace("ContainerCreating", "✨️ ContainerCreating")
            .to_string();

    Ok(pods)
}

pub fn get_all(pod: &FoundPod) -> anyhow::Result<String> {
    let output = {
        Command::new("kubectl")
            .arg("get")
            .arg("all")
            .arg("-n")
            .arg(&pod.namespace)
            .arg("--no-headers")
            .output()
    };

    let all = String::from_utf8(output.unwrap().stdout).unwrap().to_string();

    Ok(all)
}

pub fn edit_deployment(pod: &FoundPod) -> anyhow::Result<()> {
    Command::new("kubectl")
            .arg("edit")
            .arg("deployment")
            .arg(&pod.deployment)
            .arg("-n")
            .arg(&pod.namespace)
            .spawn()
            .unwrap()
            .wait()
            .expect("failed to execute process");

    Ok(())
}

pub fn get_pod_logs(pod: &FoundPod, lite: bool, last_container: bool) -> anyhow::Result<String> {
    let output = {
        Command::new("kubectl")
            .arg("logs")
            .arg(&pod.name)
            .arg("-n")
            .arg(&pod.namespace)
            .arg(if lite {"--tail=500"} else {"--tail=-1"})
            .arg(if last_container {"--previous=true"} else {"--previous=false"})
            .output()
            .expect("failed to execute process")
    };

    let logs = String::from_utf8(output.stdout).unwrap().to_string();

    Ok(logs)
}

pub fn describe_pod(pod: &FoundPod) -> anyhow::Result<String> {
    let output = {
        Command::new("kubectl")
            .arg("describe")
            .arg("pod")
            .arg(&pod.name)
            .arg("-n")
            .arg(&pod.namespace)
            .output()
            .expect("failed to execute process")
    };

    let describe = String::from_utf8(output.stdout).unwrap().to_string();

    Ok(describe)
}