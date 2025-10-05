use std::env;

fn main() {
    let repo_url = env::var("CARGO_PKG_REPOSITORY").unwrap_or_default();

    if repo_url.starts_with("https://github.com/") {
        let parts: Vec<&str> = repo_url.trim_end_matches(".git").split('/').collect();
        if let (Some(owner), Some(name)) = (parts.get(parts.len() - 2), parts.get(parts.len() - 1)) {
            println!("cargo:rustc-env=REPO_OWNER={}", owner);
            println!("cargo:rustc-env=REPO_NAME={}", name);
        }
    }
}