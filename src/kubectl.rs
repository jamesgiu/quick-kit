use std::process::{Command, Stdio};

use color_eyre::eyre::{Context, Result};
use regex::Regex;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum KubeError {
    #[error("Resource not found with provided matcher: {0} in namespace {1}")]
    ResourceNotFoundError(String, String),
}

#[derive(Default)]
pub struct FoundPod {
    pub name: String,
    pub namespace: String,
    pub deployment: String,
}

pub fn find_matching_deployment(matcher: &str, namespace: &str) -> Result<String> {
    let deployment_output = Command::new("kubectl")
        .arg("get")
        .arg("deployments")
        .arg("-n")
        .arg(&namespace)
        .output()
        .wrap_err("Could not get deployments")?;

    let deployments = String::from_utf8(deployment_output.stdout)?;

    let sanitised_matcher = Regex::new(r"\-+[0-9]+")?
        .replace_all(matcher, "")
        .to_string();

    let re = Regex::new(&format!(r"[A-Za-z-]*{sanitised_matcher}[A-Za-z-]* "))?;

    match re.captures(&deployments) {
        Some(matches) => {
            let deployment: String = matches[0].to_string().replace(" ", "");
            Ok(deployment)
        }
        None => Err(KubeError::ResourceNotFoundError(
            sanitised_matcher,
            namespace.to_string(),
        )
        .into()),
    }
}

pub fn find_matching_pod(matcher: &str) -> Result<FoundPod> {
    let output = Command::new("kubectl")
        .arg("get")
        .arg("pods")
        .arg("--all-namespaces")
        .output()
        .wrap_err("Could not execute kubectl get pods")?;

    let pods = String::from_utf8(output.stdout)?;

    let re = Regex::new(&format!(
        r"(\b.*\b)( .*{matcher}.*-[0-9A-Za-z-]+)"
    ))?;

    match re.captures(&pods) {
        Some(matches) => {
            let pod = matches
                .get(2)
                .ok_or_else(|| color_eyre::eyre::eyre!("No pod name match found"))?
                .as_str()
                .replace(" ", "");
            let ns = matches
                .get(1)
                .ok_or_else(|| color_eyre::eyre::eyre!("No namespace match found"))?
                .as_str()
                .to_string();
            let deployment = find_matching_deployment(&matcher, &ns)?;

            Ok(FoundPod {
                name: pod,
                namespace: ns,
                deployment,
            })
        }
        None => Err(KubeError::ResourceNotFoundError(
            matcher.to_string(),
            "all".to_string(),
        )
        .into()),
    }
}

pub fn debug_pod(pod: &FoundPod) -> Result<()> {
    let image_name = String::from_utf8(
        Command::new("kubectl")
            .arg("get")
            .arg("pod")
            .arg(&pod.name)
            .arg("-n")
            .arg(&pod.namespace)
            .arg("-o=jsonpath={.spec.containers[0].image}")
            .output()
            .wrap_err("Failed to get image name")?
            .stdout,
    )
    .wrap_err("Invalid UTF-8 in image name")?
    .replace("[ ", "");

    let container_name = String::from_utf8(
        Command::new("kubectl")
            .arg("get")
            .arg("pod")
            .arg(&pod.name)
            .arg("-n")
            .arg(&pod.namespace)
            .arg("-o=jsonpath={.spec.containers[0].name}")
            .output()
            .wrap_err("Failed to get container name")?
            .stdout,
    )
    .wrap_err("Invalid UTF-8 in container name")?
    .replace("[ ", "");

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
        .wrap_err("Failed to spawn kubectl debug")?
        .wait()
        .wrap_err("Failed to wait for kubectl debug")?;

    Ok(())
}

pub fn exec_into_pod(pod: &FoundPod) -> Result<()> {
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
        .wrap_err("Failed to exec into pod")?
        .wait()
        .wrap_err("Failed to wait for exec command")?;

    Ok(())
}

pub fn delete_pod(pod: &FoundPod) -> Result<String> {
    let output = Command::new("kubectl")
        .arg("delete")
        .arg("pod")
        .arg(&pod.name)
        .arg("-n")
        .arg(&pod.namespace)
        .arg("--wait=false")
        .output()
        .wrap_err("Failed to delete pod")?;

    let delete = String::from_utf8(output.stdout)?;
    Ok(delete)
}

pub fn get_pods(pod: &FoundPod) -> Result<String> {
    let output = Command::new("kubectl")
        .arg("get")
        .arg("pods")
        .arg("-n")
        .arg(&pod.namespace)
        .arg("--sort-by=.status.startTime")
        .arg("--no-headers")
        .stdout(Stdio::piped())
        .spawn()
        .wrap_err("Failed to run kubectl get pods")?;

    let tac = Command::new("tac")
        .stdin(output.stdout.ok_or_else(|| {
            color_eyre::eyre::eyre!("Failed to capture stdout from kubectl get pods")
        })?)
        .output()
        .wrap_err("Failed to run tac on pods output")?;

    let pods = String::from_utf8(tac.stdout)
        .wrap_err("Invalid UTF-8 in pods output")?
        .replace("Running", "🏃 Running")
        .replace("Error", "❌ Error")
        .replace("Completed", "✅ Completed")
        .replace("Terminating", "💀️ Terminating")
        .replace("CrashLoopBackOff", "🔥 CrashLoopBackOff")
        .replace("ImagePullBackOff", "👻 ImagePullBackOff")
        .replace("ContainerCreating", "✨️ ContainerCreating");

    Ok(pods)
}

pub fn get_all(pod: &FoundPod) -> Result<String> {
    let output = Command::new("kubectl")
        .arg("get")
        .arg("all")
        .arg("-n")
        .arg(&pod.namespace)
        .arg("--no-headers")
        .output()
        .wrap_err("Failed to get all resources")?;

    let all = String::from_utf8(output.stdout)?;
    Ok(all)
}

pub fn edit_deployment(pod: &FoundPod) -> Result<()> {
    Command::new("kubectl")
        .arg("edit")
        .arg("deployment")
        .arg(&pod.deployment)
        .arg("-n")
        .arg(&pod.namespace)
        .spawn()
        .wrap_err("Failed to spawn kubectl edit")?
        .wait()
        .wrap_err("Failed to wait for edit process")?;

    Ok(())
}

pub fn get_pod_logs(pod: &FoundPod, lite: bool, last_container: bool) -> Result<String> {
    let output = Command::new("kubectl")
        .arg("logs")
        .arg(&pod.name)
        .arg("-n")
        .arg(&pod.namespace)
        .arg(if lite { "--tail=500" } else { "--tail=-1" })
        .arg(if last_container {
            "--previous=true"
        } else {
            "--previous=false"
        })
        .output()
        .wrap_err("Failed to get pod logs")?;

    let logs = String::from_utf8(output.stdout)?;
    Ok(logs)
}

pub fn describe_pod(pod: &FoundPod) -> Result<String> {
    let output = Command::new("kubectl")
        .arg("describe")
        .arg("pod")
        .arg(&pod.name)
        .arg("-n")
        .arg(&pod.namespace)
        .output()
        .wrap_err("Failed to describe pod")?;

    let describe = String::from_utf8(output.stdout)?;
    Ok(describe)
}
