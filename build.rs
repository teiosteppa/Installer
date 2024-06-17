use pelite::resources::version_info::{Language, VersionInfo};

fn read_pe_version_info<'a>(image: &'a [u8]) -> Option<VersionInfo<'a>> {
    pelite::PeFile::from_bytes(image).ok()?.resources().ok()?.version_info().ok()
}

const LANG_NEUTRAL_UNICODE: Language = Language { lang_id: 0x0000, charset_id: 0x04b0 };
fn detect_hachimi_version() {
    println!("cargo:rerun-if-env-changed=HACHIMI_VERSION");

    // Allow manual override
    if std::env::var("HACHIMI_VERSION").is_ok() {
        return;
    }

    let map = pelite::FileMap::open("hachimi.dll").expect("hachimi.dll in project root");
    let version_info = read_pe_version_info(map.as_ref()).expect("version info in hachimi.dll");
    println!(
        "cargo:rustc-env=HACHIMI_VERSION={}",
        version_info.value(LANG_NEUTRAL_UNICODE, "ProductVersion").expect("ProductVersion in version info")
    );
}

fn compile_resources() {
    println!("cargo:rerun-if-changed=assets");

    let version_info_str = env!("CARGO_PKG_VERSION");
    let version_info_ver = format!(
        "{},{},{},0",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH")
    );
    embed_resource::compile(
        "assets/installer.rc",
        &[
            format!("VERSION_INFO_STR=\"{}\"", version_info_str),
            format!("VERSION_INFO_VER={}", version_info_ver)
        ]
    );
}

fn main() {
    detect_hachimi_version();
    compile_resources();
}