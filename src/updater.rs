use chrono::{DateTime, Utc};
use std::env;
use std::fs::{self, File};
use reqwest::header::ACCEPT;
use tempfile::Builder;
use crate::i18n::t;

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
        Err(e) => return UpdateStatus::Failed(t!("details.update_error.exe_path", error = e.to_string())),
    };
    let local_metadata = match fs::metadata(&current_exe_path) {
        Ok(md) => md,
        Err(e) => return UpdateStatus::Failed(t!("details.update_error.exe_metadata", error = e.to_string())),
    };
    let local_modified_time: DateTime<Utc> = match local_metadata.modified() {
        Ok(time) => time.into(),
        Err(e) => return UpdateStatus::Failed(t!("details.update_error.exe_modtime", error = e.to_string())),
    };

    let releases = match self_update::backends::github::ReleaseList::configure()
        .repo_owner(repo_owner)
        .repo_name(repo_name)
        .build()
        .and_then(|builder| builder.fetch())
    {
        Ok(releases) => releases,
        Err(e) => return UpdateStatus::Failed(t!("details.update_error.fetch_releases", error = e.to_string())),
    };

    let nightly_release = match releases.iter().find(|r| r.version == RELEASE_TAG) {
        Some(release) => release,
        None => return UpdateStatus::Failed(t!("details.update_error.no_release_tag", tag = RELEASE_TAG)),
    };

    let asset = match nightly_release.assets.iter().find(|a| a.name == bin_name) {
        Some(asset) => asset,
        None => return UpdateStatus::Failed(t!("details.update_error.no_asset", asset_name = bin_name, tag = RELEASE_TAG)),
    };

    if nightly_release.date.is_empty() {
        return UpdateStatus::Failed(t!("details.update_error.no_release_date"));
    }

    let remote_published_at = match DateTime::parse_from_rfc3339(&nightly_release.date) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(e) => return UpdateStatus::Failed(t!("details.update_error.parse_timestamp", error = e.to_string())),
    };

    if remote_published_at > local_modified_time {
        println!("{}", t!("cli.update_status.found_newer"));

        let tmp_dir = match Builder::new().prefix("self_update").tempdir_in(env::current_dir().unwrap()) {
            Ok(dir) => dir,
            Err(e) => return UpdateStatus::Failed(t!("details.update_error.temp_dir", error = e.to_string())),
        };
        let new_exe_path = tmp_dir.path().join(&asset.name);
        let new_exe_file = match File::create(&new_exe_path) {
            Ok(file) => file,
            Err(e) => return UpdateStatus::Failed(t!("details.update_error.temp_file", error = e.to_string())),
        };

        match self_update::Download::from_url(&asset.download_url)
            .set_header(ACCEPT, "application/octet-stream".parse().unwrap())
            .show_progress(true)
            .download_to(new_exe_file) {
                Ok(_) => (),
                Err(e) => return UpdateStatus::Failed(t!("details.update_error.download", error = e.to_string())),
        }

        match self_update::self_replace::self_replace(&new_exe_path) {
            Ok(_) => UpdateStatus::Updated(remote_published_at.to_rfc2822()),
            Err(e) => UpdateStatus::Failed(t!("details.update_error.self_replace", error = e.to_string())),
        }
    } else {
        UpdateStatus::NotNeeded
    }
}