use std::{fs, process::Command};
use color_eyre::{Result};
use crate::kubectl::{self, FoundPod};

pub fn open_in_vim(pod: &FoundPod) -> Result<()> {
    let logs = kubectl::get_pod_logs(pod, false, false).unwrap();
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