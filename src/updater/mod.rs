use std::{fs::File, io::Write};

use color_eyre::eyre::{Error, Result};
use reqwest::{Client};
use serde::Deserialize;
use reqwest::header::USER_AGENT;
use zip::ZipArchive;


#[derive(Deserialize)]

struct Asset {
    browser_download_url: String
}

#[derive(Deserialize)]
struct LatestReleaseMeta {
    assets: Vec<Asset>
}

///
/// Downloads latest version of QuickKit available for installation.
pub async fn download_latest() -> Result<(), Error> {
    
    let releases_url = "https://api.github.com/repos/jamesgiu/quick-kit/releases/latest";

    let client = Client::new();

    let response = client
    .get(releases_url)
    .header(USER_AGENT, "quick-kit")
    .send()
    .await?
    .json::<LatestReleaseMeta>()
    .await?;


    let download_url = &response.assets[0].browser_download_url;

    let download_bytes = client
    .get(download_url)
    .header(USER_AGENT, "quick-kit")
    .send()
    .await?
    .bytes()
    .await?;

    let temp_path = "/tmp/qk.zip";
    let mut zip_file_write = File::create(&temp_path)?;
    zip_file_write.write_all(&download_bytes)?;

    println!("Download complete.");

    // Extract
    let zip_file_read = File::open(temp_path)?;
    let mut archive = ZipArchive::new(zip_file_read)?;
    archive.extract("/home/jjg/.local/bin/")?;

    println!("Extraction complete.");

    

    

    // let releases_json = reqwest::get(releases_url)
    // .await?
    // .json::<LatestReleaseMeta>()
    // .await?;

    // let releases_json = reqwest::get(releases_url).await?.text().await?;

    // println!("{:?}", releases_json);
    // FIXME error with line
    println!("{:?}", response.assets[0].browser_download_url);

    Ok(())
}


