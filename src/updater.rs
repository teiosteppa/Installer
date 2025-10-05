use chrono::{DateTime, Utc};
use self_update::cargo_crate_version;
use std::env;
use std::fs;

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

    if !nightly_release.assets.iter().any(|a| a.name == bin_name) {
        return UpdateStatus::Failed(format!("No asset named '{}' found in the '{}' release.", bin_name, RELEASE_TAG));
    }

    if nightly_release.date.is_empty() {
        return UpdateStatus::Failed("Nightly release is missing a publication date.".to_string());
    }

    let remote_published_at = match DateTime::parse_from_rfc3339(&nightly_release.date) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(e) => return UpdateStatus::Failed(format!("Failed to parse remote release timestamp: {}", e)),
    };

    if remote_published_at > local_modified_time {
        println!("Newer nightly build found. Updating...");
        match self_update::backends::github::Update::configure()
            .repo_owner(repo_owner)
            .repo_name(repo_name)
            .bin_name(bin_name)
            .current_version(cargo_crate_version!())
            .target_version_tag(RELEASE_TAG)
            .build()
            .and_then(|updater| updater.update())
        {
            Ok(_) => UpdateStatus::Updated(remote_published_at.to_rfc2822()),
            Err(e) => UpdateStatus::Failed(format!("Update failed during download/install: {}", e)),
        }
    } else {
        UpdateStatus::NotNeeded
    }
}