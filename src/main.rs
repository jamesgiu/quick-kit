mod kubectl;
mod cli;
mod gui;
mod updater;

use color_eyre::{config::HookBuilder, eyre::{Error, Result}};
use clap::Parser;

use crate::kubectl::KubectlRunnerAgent;

/// Program to execute kubectl commands on resources, using regex matching.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, help="--update to download and install the newest version of Quick-Kit", conflicts_with="matcher")]
    update: bool,
    #[arg(index = 1, help="my-pod-matcher, e.g. 'nginx' for 'nginx-controller-abc123-abc'")]
    matcher: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    HookBuilder::default()
    .display_env_section(true)  // remove env advice
    .panic_section(true)        // remove panic section
    .display_location_section(true) // THIS hides file:line info
    .install()?;

    let args = Args::parse();

    if args.update {
        updater::download_latest().await?
    }

    if let Some(matcher_string) = args.matcher {
        let pod = kubectl::find_matching_pod(&KubectlRunnerAgent{}, matcher_string.as_str())?;
        gui::gui(pod)?
    }

    Ok(())
}
