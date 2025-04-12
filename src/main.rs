mod klog;

use anyhow::{Result};
use clap::Parser;
use std::process::{Command};
use regex::{Captures, Regex};
use klog::klog;

/// Program to execute kubectl commands on resources, using regex matching.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(index = 1, help="my-pod-matcher, e.g. 'nginx' for 'nginx-controller-abc123-abc'")]
    matcher: String
}

#[derive(Default)]
struct FoundPod {
    name: String,
    namespace: String,
    deployment: String
}

fn main() -> Result<()> {
    let args = Args::parse();

    let pod = find_matching_pod(&args.matcher).unwrap();
    klog(pod).unwrap();

    Ok(())
}

fn find_matching_pod(matcher: &str) -> Result<FoundPod> {
    let output = {
        Command::new("kubectl")
            .arg("get")
            .arg("pods")
            .arg("--all-namespaces")
            .output()
            .expect("failed to execute process")
    };

    let pods = String::from_utf8(output.stdout).unwrap().to_string();

    let mut re = Regex::new(&format!(r"(\b.*\b)( .*{matcher}.*-[0-9A-Za-z-]+)")).unwrap();

    // First match will be namespace, second will be pod
    let Some(matches): Option<Captures> = re.captures(&*pods) else { todo!() };

    let pod: String = matches[2].replace(" ", "");
    let ns: String = matches[1].to_string();

    let deployment_output = {
        Command::new("kubectl")
            .arg("get")
            .arg("deployments")
            .arg("-n")
            .arg(&ns)
            .output()
            .expect("failed to execute process")
    };

    let deployments = String::from_utf8(deployment_output.stdout).unwrap().to_string();

    re = Regex::new(&format!(r".*{matcher}.*[A-Za-z]+ ")).unwrap();

    let Some(deployment_matches): Option<Captures> = re.captures(&*deployments) else { todo!() };

    let deployment: String = deployment_matches[0].to_string().replace(" ", "");

    let found_pod : FoundPod = FoundPod {
        name: pod,
        namespace: ns,
        deployment: deployment
    };

    Ok(found_pod)
}
