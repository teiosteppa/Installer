use chrono::{DateTime, Utc};
use std::env;
use std::fs::{self, File};
use reqwest::header::ACCEPT;
use tempfile::Builder;

#[derive(Debug)]
pub enum UpdateStatus {
    Updated(String),
    NotNeeded,
    Failed(String),
    Disabled,
}

const RELEASE_TAG: &str = "nightly";

pub fn run_update_check() -> UpdateStatus {
    let repo_owner = option_env!("REPO_OWNER");
    let repo_name = option_env!("REPO_NAME");

    if repo_owner.is_none() || repo_name.is_none() {
        return UpdateStatus::Disabled;
    }
    let repo_owner = repo_owner.unwrap();
    let repo_name = repo_name.unwrap();

    let bin_name = "hachimi_installer.exe";

    let current_exe_path = match env::current_exe() {
        Ok(path) => path,
        Err(e) => return UpdateStatus::Failed(format!("Could not get current executable path: {}", e)),
    };
    let local_metadata = match fs::metadata(&current_exe_path) {
        Ok(md) => md,
        Err(e) => return UpdateStatus::Failed(format!("Could not get metadata for current executable: {}", e)),
    };
    let local_modified_time: DateTime<Utc> = match local_metadata.modified() {
        Ok(time) => time.into(),
        Err(e) => return UpdateStatus::Failed(format!("Could not read modification time of executable: {}", e)),
    };

    let releases = match self_update::backends::github::ReleaseList::configure()
        .repo_owner(repo_owner)
        .repo_name(repo_name)
        .build()
        .and_then(|builder| builder.fetch())
    {
        Ok(releases) => releases,
        Err(e) => return UpdateStatus::Failed(format!("Failed to fetch releases from GitHub: {}", e)),
    };

    let nightly_release = match releases.iter().find(|r| r.version == RELEASE_TAG) {
        Some(release) => release,
        None => return UpdateStatus::Failed(format!("No release with tag '{}' found.", RELEASE_TAG)),
    };

    let asset = match nightly_release.assets.iter().find(|a| a.name == bin_name) {
        Some(asset) => asset,
        None => return UpdateStatus::Failed(format!("No asset named '{}' found in the '{}' release.", bin_name, RELEASE_TAG)),
    };

    if nightly_release.date.is_empty() {
        return UpdateStatus::Failed("Nightly release is missing a publication date.".to_string());
    }

    let remote_published_at = match DateTime::parse_from_rfc3339(&nightly_release.date) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(e) => return UpdateStatus::Failed(format!("Failed to parse remote release timestamp: {}", e)),
    };

    if remote_published_at > local_modified_time {
        println!("Newer nightly build found. Updating...");

        let tmp_dir = match Builder::new().prefix("self_update").tempdir_in(env::current_dir().unwrap()) {
            Ok(dir) => dir,
            Err(e) => return UpdateStatus::Failed(format!("Failed to create temp dir: {}", e)),
        };
        let new_exe_path = tmp_dir.path().join(&asset.name);
        let new_exe_file = match File::create(&new_exe_path) {
            Ok(file) => file,
            Err(e) => return UpdateStatus::Failed(format!("Failed to create temp file: {}", e)),
        };
        
        match self_update::Download::from_url(&asset.download_url)
            .set_header(ACCEPT, "application/octet-stream".parse().unwrap()) // Add this line
            .show_progress(true)
            .download_to(new_exe_file) {
                Ok(_) => (),
                Err(e) => return UpdateStatus::Failed(format!("Failed to download new release: {}", e)),
        }

        match self_update::self_replace::self_replace(&new_exe_path) {
            Ok(_) => UpdateStatus::Updated(remote_published_at.to_rfc2822()),
            Err(e) => UpdateStatus::Failed(format!("Update failed during install: {}", e)),
        }
    } else {
        UpdateStatus::NotNeeded
    }
}