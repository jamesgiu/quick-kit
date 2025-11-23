use std::{io::Write, process::{Command, Stdio}};

use color_eyre::eyre::{Context, Result};
use regex::Regex;
use thiserror::Error;

pub trait KubectlRunner {
    fn run_commands(&self, args: &[&str]) -> Result<String>;
    fn spawn_shell(&self, args: &[&str]) -> Result<()>;
}

pub struct KubectlRunnerAgent;

impl KubectlRunner for KubectlRunnerAgent {
    fn run_commands(&self, args: &[&str]) -> Result<String> {
        let output = String::from_utf8(Command::new("kubectl")
        .args(args)
        .output()
        .wrap_err("Could not run commands")?.stdout)?;

        Ok(output)
    }

    fn spawn_shell(&self, args: &[&str]) -> Result<()> {
       Command::new("kubectl")
        .args(args)
        .spawn()
        .wrap_err("Could not run commands")?
        .wait()
        .wrap_err("could not spawn process")?;

        Ok(())
    }
} 

/// Custom error type for Kubernetes resource matching operations.
#[derive(Error, Debug)]
pub enum KubeError {
    /// Raised when a resource could not be found for a given matcher in a specified namespace.
    #[error("Resource not found with provided matcher: {0} in namespace {1}")]
    ResourceNotFoundError(String, String),
    #[error("Execution not able to be performed on {0} in namespace {1}")]
    ResourceExecutionIssue(String, String)
}

/// Represents a Kubernetes pod and its associated metadata.
#[derive(Default)]
pub struct FoundPod {
    /// Name of the pod.
    pub name: String,
    /// Namespace where the pod is located.
    pub namespace: String,
    /// Name of the deployment managing the pod.
    pub deployment: String,
}

/// Attempts to find a matching Kubernetes deployment based on a matcher string and namespace.
///
/// This function uses `kubectl get deployments` and regex matching to find a relevant deployment.
///
/// # Arguments
/// * `matcher` - A string to match the deployment name.
/// 
/// * `namespace` - The Kubernetes namespace to search in.
///
/// # Errors
/// Returns an error if `kubectl` fails, the output is invalid UTF-8, or no deployment is found.
pub fn find_matching_deployment(runner: &dyn KubectlRunner, matcher: &str, namespace: &str) -> Result<String> {
    let deployments = runner.run_commands(&["get", "deployments", "-n", namespace])?;

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

/// Finds a pod by using a matcher string across all namespaces.
///
/// # Arguments
/// * `matcher` - A string used to locate a matching pod.
///
/// # Returns
/// A `FoundPod` struct containing the pod name, namespace, and owning deployment.
///
/// # Errors
/// Returns an error if the pod or its deployment cannot be found.
pub fn find_matching_pod(runner: &dyn KubectlRunner, matcher: &str) -> Result<FoundPod> {
    let pods = runner.run_commands(&["get", "pods", "--all-namespaces"])?;
    
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
            let deployment = find_matching_deployment(runner, &matcher, &ns)?;

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

/// Spawns a debug container into the given pod using the same image and container name.
///
/// # Arguments
/// * `pod` - A reference to the `FoundPod` struct representing the target pod.
///
/// # Errors
/// Returns an error if `kubectl debug` or the underlying metadata fetch commands fail.
pub fn debug_pod(runner: &dyn KubectlRunner, pod: &FoundPod) -> Result<()> {
    let image_name = runner.run_commands(&[
        "get", "pod", &pod.name, "-n", &pod.namespace,
        "-o=jsonpath={.spec.containers[0].image}",
    ])?;

    let container_name = runner.run_commands(&[
        "get", "pod", &pod.name, "-n", &pod.namespace,
        "-o=jsonpath={.spec.containers[0].name}",
    ])?;

    runner.spawn_shell(&[
        "debug", &pod.name, "-n", &pod.namespace, "-it",
        &format!("--image={}", image_name),
        &format!("--target={}", container_name),
        "--", "sh",
    ])
}

/// Starts an interactive shell session inside a running pod container.
///
/// # Arguments
/// * `pod` - A reference to the target `FoundPod`.
///
/// # Errors
/// Returns an error if the `kubectl exec` command fails.
pub fn exec_into_pod(runner: &dyn KubectlRunner, pod: &FoundPod) -> Result<()> {
    runner.spawn_shell(&[
        "exec", "--stdin", "--tty", &pod.name, "-n", &pod.namespace, "--", "/bin/sh",
    ])
}

/// Deletes the given pod without waiting for completion.
///
/// # Arguments
/// * `pod` - A reference to the pod to delete.
///
/// # Returns
/// A string output of the `kubectl delete` command.
///
/// # Errors
/// Returns an error if the command fails or the output can't be decoded.
pub fn delete_pod(runner: &dyn KubectlRunner, pod: &FoundPod) -> Result<String> {
    runner.run_commands(&[
        "delete", "pod", &pod.name, "-n", &pod.namespace, "--wait=false",
    ])
}

/// Retrieves a reversed and formatted list of pods sorted by start time in the given namespace.
///
/// # Arguments
/// * `pod` - A reference to the namespace's pod (only namespace field is used).
///
/// # Returns
/// A formatted string of pod statuses.
///
/// # Errors
/// Returns an error if the `kubectl` or `tac` commands fail or output can't be parsed.
pub fn get_pods(runner: &dyn KubectlRunner, pod: &FoundPod) -> Result<String> {
    let pods_output = runner.run_commands(&[
        "get", "pods", "-n", &pod.namespace,
        "--sort-by=.status.startTime", "--no-headers",
    ])?;

    let mut tac = Command::new("tac")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .wrap_err("Failed to run tac")?;

    if let Some(stdin) = tac.stdin.as_mut() {
        stdin.write_all(pods_output.as_bytes())?;
    }

    let output = tac.wait_with_output().wrap_err("Failed to get tac output")?;

    let pods = String::from_utf8(output.stdout)?
        .replace("Running", "🏃 Running")
        .replace("Error", "❌ Error")
        .replace("Completed", "✅ Completed")
        .replace("Terminating", "💀️ Terminating")
        .replace("CrashLoopBackOff", "🔥 CrashLoopBackOff")
        .replace("ImagePullBackOff", "👻 ImagePullBackOff")
        .replace("ContainerCreating", "✨️ ContainerCreating");

    Ok(pods)
}

/// Lists all resources in the pod's namespace (no headers).
///
/// # Arguments
/// * `pod` - A reference to the pod (only namespace is used).
///
/// # Returns
/// Output of `kubectl get all`.
///
/// # Errors
/// Returns an error if the command fails or output is invalid.
pub fn get_all(runner: &dyn KubectlRunner, pod: &FoundPod) -> Result<String> {
    runner.run_commands(&["get", "all", "-n", &pod.namespace, "--no-headers"])
}

/// Opens the deployment of the given pod in an editor.
///
/// # Arguments
/// * `pod` - The pod whose deployment should be edited.
///
/// # Errors
/// Returns an error if `kubectl edit` fails to spawn or complete.
pub fn edit_deployment(runner: &dyn KubectlRunner, pod: &FoundPod) -> Result<()> {
    runner.spawn_shell(&[
        "edit", "deployment", &pod.deployment, "-n", &pod.namespace,
    ])
}

/// Fetches logs from a given pod, optionally from the last container or limiting output.
///
/// # Arguments
/// * `pod` - The pod to retrieve logs from.
/// * `lite` - If `true`, limits to last 500 lines.
/// * `last_container` - If `true`, fetches logs from the previous container instance.
///
/// # Returns
/// The logs as a string.
///
/// # Errors
/// Returns an error if the command fails or output is not UTF-8.
///
/// # Example
/// ```no_run
/// let pod = find_matching_pod("api")?;
/// let logs = get_pod_logs(&pod, true, false)?;
/// println!("{}", logs);
/// # Ok::<(), color_eyre::eyre::Report>(())
/// ```
pub fn get_pod_logs(runner: &dyn KubectlRunner, pod: &FoundPod, lite: bool, last_container: bool) -> Result<String> {
    let output = runner.run_commands(
        &["logs", &pod.name, "-n", &pod.namespace, if lite { "--tail=500" } else { "--tail=-1" }, if last_container {
            "--previous=true"
        } else {
            "--previous=false"
        }]);

    match output {
        Ok(logs) => {
            Ok(logs)
        },
        Err(err) => {
            Err(err.wrap_err(KubeError::ResourceExecutionIssue(pod.name.to_string(), pod.namespace.to_string())))
        }
    }

}

/// Describes the given pod using `kubectl describe`.
///
/// # Arguments
/// * `pod` - The pod to describe.
///
/// # Returns
/// The full description string.
///
/// # Errors
/// Returns an error if the command fails or the output can't be decoded.
pub fn describe_pod(runner: &dyn KubectlRunner, pod: &FoundPod) -> Result<String> {
    runner.run_commands(&["describe", "pod", &pod.name, "-n", &pod.namespace])
}

#[cfg(test)]
mod tests {
    use color_eyre::eyre;
    use crate::kubectl::{FoundPod, delete_pod, describe_pod, exec_into_pod, get_all, get_pod_logs, tests::eyre::eyre};
    use color_eyre::eyre::{Result};

    use crate::kubectl::{find_matching_deployment, find_matching_pod, KubeError, KubectlRunner};

    const EXPECTED_ERROR: &str = "error";

    static mut COUNTER: usize = 0;

    #[derive(Default)]

    pub struct TestKubeCtlRunner<'a> {
        expected_args: Vec<&'a [&'a str]>,
        pod_output: Option<&'a str>,
    }

    pub struct ErroringTestKubeCtlRunner<'a> {
        expected_args: &'a [&'a str],
    }

    impl KubectlRunner for TestKubeCtlRunner<'_> {
        fn run_commands(&self, args: &[&str]) -> Result<String> {
            unsafe { assert_eq!(args, self.expected_args[COUNTER]) };
            unsafe { COUNTER += 1 };

            // Below examples are more sophistacted as they are required for chaining calls/substringing.
            if args.contains(&"pods") {
                Ok(String::from(
                    "namespace api-server-hello-123456\nnamespace2 something-else-abc",
                ))
            } else if args.contains(&"deployments") {
                Ok(String::from(
                    "NAME               READY   UP-TO-DATE   AVAILABLE   AGE\nahoy-api-server   2/2     2            2           100d",
                ))
            } else {
                Ok(self.pod_output.unwrap_or("").to_string())
            }
        }

        fn spawn_shell(&self, args: &[&str]) -> Result<()> {
            unsafe { assert_eq!(args, self.expected_args[COUNTER]) };
            unsafe { COUNTER += 1 };
            
            Ok(())
        }
    }

    impl KubectlRunner for ErroringTestKubeCtlRunner<'_> {
        fn run_commands(&self, args: &[&str]) -> Result<String> {
            assert_eq!(args, self.expected_args);
            Err(eyre!(EXPECTED_ERROR))
        }
        
        fn spawn_shell(&self, args: &[&str]) -> Result<()> {
            assert_eq!(args, self.expected_args);
            Err(eyre!(EXPECTED_ERROR))
        }
    }

    #[test]
    fn test_find_matching_deployment_success() {
        unsafe { COUNTER = 0 };
        let matcher = "api";
        let namespace = "namespace";
        let matched_result = find_matching_deployment(
            &mut TestKubeCtlRunner {
                expected_args: vec!(&["get", "deployments", "-n", namespace]),
                pod_output: None
            },
            matcher,
            namespace,
        )
        .unwrap();
        assert_eq!("ahoy-api-server", matched_result);
    }

    #[test]
    fn test_find_matching_deployment_failure() {
        unsafe { COUNTER = 0 };
        let matcher = "goodbye";
        let namespace = "namespace";
        let matched_result = find_matching_deployment(
            &mut TestKubeCtlRunner {
                expected_args: vec!(&["get", "deployments", "-n", namespace]),
                pod_output: None
            },
            matcher,
            namespace,
        );
        assert!(matched_result.is_err());
        assert_eq!(
            KubeError::ResourceNotFoundError(matcher.to_string(), namespace.to_string()).to_string(),
            matched_result.err().unwrap().to_string()
        )
    }

    #[test]
    fn test_find_matching_deployment_err() {
        unsafe { COUNTER = 0 };
        let matcher = "goodbye";
        let namespace = "namespace";
        let matched_result = find_matching_deployment(
            &mut ErroringTestKubeCtlRunner {
                expected_args: &["get", "deployments", "-n", namespace],
            },
            matcher,
            namespace,
        );
        assert!(matched_result.is_err());
        assert_eq!(EXPECTED_ERROR, matched_result.err().unwrap().to_string())
    }

    #[test]
    fn test_find_matching_pod_success() {
        unsafe { COUNTER = 0 };
        let matcher = "api-server";
        let matched_result = find_matching_pod(&mut TestKubeCtlRunner {
            expected_args: vec!(&["get", "pods", "--all-namespaces"], &["get", "deployments", "-n", "namespace"]),
            pod_output: None,
        }, matcher)
        .unwrap();

        assert_eq!(matched_result.name, "api-server-hello-123456");
        assert_eq!(matched_result.namespace, "namespace");
        assert_eq!(matched_result.deployment, "ahoy-api-server");
    }

    #[test]
    fn test_find_matching_pod_not_found() {
        unsafe { COUNTER = 0 };
        let matcher = "nonexistent";

        let result = find_matching_pod(&mut TestKubeCtlRunner {
            expected_args: vec!(&["get", "pods", "--all-namespaces"], &["get", "deployments", "-n", "namespace"]),
            pod_output: Some("namespace pod-abc\nnamespace2 something-else"),
            ..Default::default()
        }, matcher);

        assert!(result.is_err());
        assert_eq!(
            KubeError::ResourceNotFoundError(matcher.to_string(), "all".to_string()).to_string(),
            result.err().unwrap().to_string()
        );
    }

    #[test]
    fn test_find_matching_pod_kubectl_error() {
        unsafe { COUNTER = 0 };
        let matcher = "error";

        let result = find_matching_pod(&mut ErroringTestKubeCtlRunner {
            expected_args: &["get", "pods", "--all-namespaces"],
        }, matcher);

        assert!(result.is_err());
        assert_eq!(EXPECTED_ERROR, result.err().unwrap().to_string());
    }

    #[test]
    fn test_get_pod_logs_success() {
        unsafe { COUNTER = 0 };
        let pod = FoundPod {
            name: "eh".to_string(),
            namespace: "namespace".to_string(),
            deployment: "eh".to_string(),
        };

        let binding = ["logs", &pod.name, "-n", &pod.namespace, "--tail=-1", "--previous=false"];
        let test_kube_ctl_runner = TestKubeCtlRunner {
            expected_args: vec!(&binding),
            pod_output: Some("these are some logs")
        };

        let result = get_pod_logs(&test_kube_ctl_runner, &pod, false, false);

        assert!(result.is_ok());
        assert_eq!("these are some logs", result.unwrap().to_string())
    }

    #[test]
    fn test_get_pod_logs_error() {
        unsafe { COUNTER = 0 };
        let pod = FoundPod {
            name: "eh".to_string(),
            namespace: "namespace".to_string(),
            deployment: "eh".to_string(),
        };

        let binding = &["logs", &pod.name, "-n", &pod.namespace, "--tail=-1", "--previous=false"];
        let test_kube_ctl_runner = ErroringTestKubeCtlRunner {
            expected_args: binding,
        };

        let result = get_pod_logs(&test_kube_ctl_runner, &pod, false, false);

        assert!(result.is_err());
        assert_eq!(KubeError::ResourceExecutionIssue(pod.name, pod.namespace).to_string(), result.err().unwrap().to_string())
    }

    #[test]
    fn test_exec_into_pod_success() {
        unsafe { COUNTER = 0 };

        let expected_pod = FoundPod {
            name: "pod".to_string(),
            namespace: "namespace".to_string(),
            deployment: "deployment".to_string()
        };

        let args = ["exec", "--stdin", "--tty", &expected_pod.name, "-n", &expected_pod.namespace, "--", "/bin/sh"];

        let test_kubectl_runner = TestKubeCtlRunner {
            expected_args: vec!(&args),
            pod_output: None,
        };

        let result = exec_into_pod(&test_kubectl_runner, &expected_pod);

        assert!(result.is_ok());
    }

    #[test]
    fn test_exec_into_pod_failure() {
        unsafe { COUNTER = 0 };

        let expected_pod = FoundPod {
            name: "pod".to_string(),
            namespace: "namespace".to_string(),
            deployment: "deployment".to_string()
        };

        let args = &["exec", "--stdin", "--tty", &expected_pod.name, "-n", &expected_pod.namespace, "--", "/bin/sh"];

        let test_kubectl_runner = ErroringTestKubeCtlRunner {
            expected_args: args
        };

        let result = exec_into_pod(&test_kubectl_runner, &expected_pod);

        assert!(result.is_err());
    }

    #[test]
    fn test_describe_pod_success() {
        unsafe { COUNTER = 0 };

        let expected_pod = FoundPod {
            name: "pod".to_string(),
            namespace: "namespace".to_string(),
            deployment: "deployment".to_string()
        };

        let args = ["describe", "pod", &expected_pod.name, "-n", &expected_pod.namespace];

        let test_kubectl_runner = TestKubeCtlRunner {
            expected_args: vec!(&args),
            pod_output: Some("some description"),
        };

        let result = describe_pod(&test_kubectl_runner, &expected_pod);

        assert!(result.is_ok());
        assert_eq!(test_kubectl_runner.pod_output.unwrap(), result.unwrap())
    }

    #[test]
    fn test_describe_pod_failure() {
        unsafe { COUNTER = 0 };

        let expected_pod = FoundPod {
            name: "pod".to_string(),
            namespace: "namespace".to_string(),
            deployment: "deployment".to_string()
        };

        let args = &["describe", "pod", &expected_pod.name, "-n", &expected_pod.namespace];

        let test_kubectl_runner = ErroringTestKubeCtlRunner {
            expected_args: args
        };

        let result = describe_pod(&test_kubectl_runner, &expected_pod);

        assert!(result.is_err());
    }

    #[test]
    fn test_get_all_pods_success() {
        unsafe { COUNTER = 0 };

        let expected_pod = FoundPod {
            name: "pod".to_string(),
            namespace: "namespace".to_string(),
            deployment: "deployment".to_string()
        };

        let args = ["get", "all", "-n", &expected_pod.namespace, "--no-headers"];

        let test_kubectl_runner = TestKubeCtlRunner {
            expected_args: vec!(&args),
            pod_output: Some("all pods"),
        };

        let result = get_all(&test_kubectl_runner, &expected_pod);

        assert!(result.is_ok());
        assert_eq!(test_kubectl_runner.pod_output.unwrap(), result.unwrap())
    }

    #[test]
    fn test_get_all_pods_failure() {
        unsafe { COUNTER = 0 };

        let expected_pod = FoundPod {
            name: "pod".to_string(),
            namespace: "namespace".to_string(),
            deployment: "deployment".to_string()
        };

        let args = &["get", "all", "-n", &expected_pod.namespace, "--no-headers"];

        let test_kubectl_runner = ErroringTestKubeCtlRunner {
            expected_args: args
        };

        let result = get_all(&test_kubectl_runner, &expected_pod);

        assert!(result.is_err());
    }

    #[test]
    fn test_delete_pod_success() {
        unsafe { COUNTER = 0 };

        let expected_pod = FoundPod {
            name: "pod".to_string(),
            namespace: "namespace".to_string(),
            deployment: "deployment".to_string()
        };

        let args = ["delete", "pod", &expected_pod.name, "-n", &expected_pod.namespace, "--wait=false"];

        let test_kubectl_runner = TestKubeCtlRunner {
            expected_args: vec!(&args),
            pod_output: None,
        };

        let result = delete_pod(&test_kubectl_runner, &expected_pod);

        assert!(result.is_ok());
    }

    #[test]
    fn test_delete_pod_failure() {
        unsafe { COUNTER = 0 };

        let expected_pod = FoundPod {
            name: "pod".to_string(),
            namespace: "namespace".to_string(),
            deployment: "deployment".to_string()
        };

        let args = &["delete", "pod", &expected_pod.name, "-n", &expected_pod.namespace, "--wait=false"];

        let test_kubectl_runner = ErroringTestKubeCtlRunner {
            expected_args: args
        };

        let result = delete_pod(&test_kubectl_runner, &expected_pod);

        assert!(result.is_err());
    }
}
