mod kubectl;
mod cli;
mod gui;

use color_eyre::{config::HookBuilder, eyre::Result};
use clap::{command, Parser};

use crate::kubectl::KubectlRunnerAgent;

/// Program to execute kubectl commands on resources, using regex matching.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(index = 1, help="my-pod-matcher, e.g. 'nginx' for 'nginx-controller-abc123-abc'")]
    matcher: String
}

fn main() -> Result<()> {
    HookBuilder::default()
    .display_env_section(false)  // remove env advice
    .panic_section(false)        // remove panic section
    .display_location_section(false) // THIS hides file:line info
    .install()?;

    let args = Args::parse();

    let pod = kubectl::find_matching_pod(&KubectlRunnerAgent{}, &args.matcher)?;

    Ok(gui::gui(pod)?)
}
