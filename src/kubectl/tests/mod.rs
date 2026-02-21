use super::*;

use color_eyre::eyre;
use crate::kubectl::{FoundPod, debug_pod, delete_pod, describe_pod, edit_deployment, exec_into_pod, get_all, get_pod_logs, get_pods, tests::eyre::eyre};
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

#[test]
fn test_debug_pod_success() {
    unsafe { COUNTER = 0 };

    let pod = FoundPod {
        name: "my-pod".to_string(),
        namespace: "my-ns".to_string(),
        deployment: "my-deploy".to_string(),
    };


    let get_image = [
            "get", "pod", &pod.name, "-n", &pod.namespace,
            "-o=jsonpath={.spec.containers[0].image}",
        ];


    let get_container = [
            "get", "pod", &pod.name, "-n", &pod.namespace,
            "-o=jsonpath={.spec.containers[0].name}",
        ];

    let debug_arg = [
            "debug", &pod.name, "-n", &pod.namespace, "-it",
            "--image=a-fake-image",
            "--target=a-fake-container",
            "--", "sh",
        ];

    let expected_calls: Vec<&[&str]> = vec![
        &get_image,
        &get_container,
        &debug_arg,
    ];

    struct DebugRunner<'a> {
        calls: Vec<&'a [&'a str]>,
    }

    impl<'a> KubectlRunner for DebugRunner<'a> {
        fn run_commands(&self, args: &[&str]) -> Result<String> {
            unsafe {
                assert_eq!(args, self.calls[COUNTER]);
                COUNTER += 1;
            }

            if args.contains(&"-o=jsonpath={.spec.containers[0].image}") {
                return Ok("a-fake-image".into());
            }
            if args.contains(&"-o=jsonpath={.spec.containers[0].name}") {
                return Ok("a-fake-container".into());
            }
            Ok(String::new())
        }

        fn spawn_shell(&self, args: &[&str]) -> Result<()> {
            unsafe {
                assert_eq!(args, self.calls[COUNTER]);
                COUNTER += 1;
            }
            Ok(())
        }
    }

    let runner = DebugRunner {
        calls: vec![
            expected_calls[0],
            expected_calls[1],
            expected_calls[2],
        ],
    };

    let result = debug_pod(&runner, &pod);

    assert!(result.is_ok());
}

#[test]
fn test_debug_pod_failure() {
    unsafe { COUNTER = 0 };

    let pod = FoundPod {
        name: "bad-pod".to_string(),
        namespace: "ns".to_string(),
        deployment: "dep".to_string(),
    };

    // First command should fail
    let args = &[
        "get", "pod", &pod.name, "-n", &pod.namespace,
        "-o=jsonpath={.spec.containers[0].image}",
    ];

    let runner = ErroringTestKubeCtlRunner {
        expected_args: args,
    };

    let result = debug_pod(&runner, &pod);

    assert!(result.is_err());
    assert_eq!(EXPECTED_ERROR, result.err().unwrap().to_string());
}

#[test]
fn test_edit_deployment_success() {
    unsafe { COUNTER = 0 };

    let pod = FoundPod {
        name: "pod".to_string(),
        namespace: "namespace".to_string(),
        deployment: "my-deploy".to_string(),
    };

    let expected = ["edit", "deployment", &pod.deployment, "-n", &pod.namespace];

    let runner = TestKubeCtlRunner {
        expected_args: vec!(&expected),
        pod_output: None,
    };

    let result = edit_deployment(&runner, &pod);

    assert!(result.is_ok());
}

#[test]
fn test_edit_deployment_failure() {
    unsafe { COUNTER = 0 };

    let pod = FoundPod {
        name: "pod".to_string(),
        namespace: "ns".to_string(),
        deployment: "my-deploy".to_string(),
    };

    let args = &["edit", "deployment", &pod.deployment, "-n", &pod.namespace];

    let runner = ErroringTestKubeCtlRunner { expected_args: args };

    let result = edit_deployment(&runner, &pod);

    assert!(result.is_err());
}

#[test]
fn test_get_pods_success() {
    pub struct GetPodsTestKubeCtlRunner<'a> {
        expected_args: Vec<&'a [&'a str]>,
        pod_output: Option<&'a str>,
    }

    impl KubectlRunner for GetPodsTestKubeCtlRunner<'_> {
        fn run_commands(&self, args: &[&str]) -> Result<String> {
            unsafe { assert_eq!(args, self.expected_args[COUNTER]) };
            unsafe { COUNTER += 1 };

            Ok(self.pod_output.unwrap_or("").to_string())
        }
        
        fn spawn_shell(&self, _args: &[&str]) -> Result<()> {
            todo!()
        }
    }

    unsafe { COUNTER = 0 };

    let pod = FoundPod {
        name: "ignored".to_string(),
        namespace: "ns".to_string(),
        deployment: "ignore".to_string(),
    };

    let kubectl_args = [
        "get", "pods", "-n", &pod.namespace,
        "--sort-by=.status.startTime", "--no-headers",
    ];

    let sample_output = "pod-a Running\npod-b Completed\npod-c Error";

    let runner = GetPodsTestKubeCtlRunner {
        expected_args: vec!(&kubectl_args),
        pod_output: Some(sample_output),
    };

    let result = get_pods(&runner, &pod).unwrap();

    // After tac: reverse order + emoji substitutions
    assert!(result.contains("🏃 Running"));
    assert!(result.contains("❌ Error"));
    assert!(result.contains("✅ Completed"));

    // Should be reversed by tac
    assert!(result.starts_with("pod-c"));
}

#[test]
fn test_get_pods_failure() {
    unsafe { COUNTER = 0 };

    let pod = FoundPod {
        name: "a".to_string(),
        namespace: "ns".to_string(),
        deployment: "b".to_string(),
    };

    let args = &[
        "get", "pods", "-n", &pod.namespace,
        "--sort-by=.status.startTime", "--no-headers",
    ];

    let runner = ErroringTestKubeCtlRunner { expected_args: args };

    let result = get_pods(&runner, &pod);

    assert!(result.is_err());
}
