use std::{fs, process::Command};
use color_eyre::{Result};
use crate::kubectl::{self, FoundPod, KubectlRunner};

pub fn open_in_vim(runner: &dyn KubectlRunner, pod: &FoundPod) -> Result<()> {
    let logs = kubectl::get_pod_logs(runner, pod, false, false).unwrap();
    let name = &pod.name;
    let fname = format!("/tmp/klog_{name}");
    fs::write(&fname, logs).expect("Unable to write file");
    let _output = {
        Command::new("vim")
            .arg(&fname)
            .spawn()
            .unwrap()
            .wait()
            .expect("failed to execute process")
    };

    Ok(())
}


