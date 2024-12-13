use std::{fs::File, io::Write, path::{Path, PathBuf}};

use pelite::resources::version_info::Language;
use tinyjson::JsonValue;
use windows::Win32::UI::Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

use crate::utils::{self, get_system_directory};

pub struct Installer {
    pub install_dir: Option<PathBuf>,
    pub target: Target,
    pub custom_target: Option<String>,
    system_dir: PathBuf
}

impl Installer {
    pub fn custom(install_dir: Option<PathBuf>, target: Option<String>) -> Installer {
        Installer {
            install_dir: install_dir.or_else(Self::detect_install_dir),
            target: Target::default(),
            custom_target: target,
            system_dir: get_system_directory()
        }
    }

    fn detect_install_dir() -> Option<PathBuf> {
        let app_data_dir_wstr = unsafe { SHGetKnownFolderPath(&FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, None).ok()? };
        let app_data_dir_str = unsafe { app_data_dir_wstr.to_string().ok()? };
        let app_data_dir = Path::new(&app_data_dir_str);
        let mut dmm_config_path = app_data_dir.join("dmmgameplayer5");
        dmm_config_path.push("dmmgame.cnf");

        let config_str = std::fs::read_to_string(dmm_config_path).ok()?;
        let JsonValue::Object(config) = config_str.parse().ok()? else {
            return None;
        };
        let JsonValue::Array(config_contents) = &config["contents"] else {
            return None;
        };
        for value in config_contents {
            let JsonValue::Object(game) = value else {
                return None;
            };

            let JsonValue::String(product_id) = &game["productId"] else {
                continue;
            };
            if product_id != "umamusume" {
                continue;
            }

            let JsonValue::Object(detail) = &game["detail"] else {
                return None;
            };
            let JsonValue::String(path_str) = &detail["path"] else {
                return None;
            };

            let path = PathBuf::from(path_str);
            return if path.is_dir() {
                Some(path)
            }
            else {
                None
            }
        }

        None
    }

    pub fn get_target_path(&self, target: Target) -> PathBuf {
        self.system_dir.join(target.dll_name())
    }

    pub fn get_current_target_path(&self) -> PathBuf {
        if let Some(custom_target) = &self.custom_target {
            self.system_dir.join(custom_target)
        }
        else {
            self.system_dir.join(self.target.dll_name())
        }
    }

    pub fn get_dest_dll_path(&self) -> Option<PathBuf> {
        Some(self.install_dir.as_ref()?.join(format!("hachimi\\{}", self.target.dll_name())))
    }

    pub fn get_src_dll_path(&self) -> Option<PathBuf> {
        Some(self.install_dir.as_ref()?.join(format!("umamusume_Data\\Plugins\\x86_64\\{}", self.target.dll_name())))
    }

    const LANG_NEUTRAL_UNICODE: Language = Language { lang_id: 0x0000, charset_id: 0x04b0 };
    pub fn get_target_version_info(&self, target: Target) -> Option<TargetVersionInfo> {
        let path = self.get_target_path(target);
        let map = pelite::FileMap::open(&path).ok()?;

        // File exists, so return empty version info if we can't read it
        let Some(version_info) = utils::read_pe_version_info(map.as_ref()) else {
            return Some(TargetVersionInfo::default());
        };

        Some(TargetVersionInfo {
            name: version_info.value(Self::LANG_NEUTRAL_UNICODE, "ProductName"),
            version: version_info.value(Self::LANG_NEUTRAL_UNICODE, "ProductVersion")
        })
    }

    pub fn get_target_display_label(&self, target: Target) -> String {
        if let Some(version_info) = self.get_target_version_info(target) {
            version_info.get_display_label(target)
        }
        else {
            target.dll_name().to_owned()
        }
    }

    pub fn is_current_target_installed(&self) -> bool {
        let path = self.get_current_target_path();

        let Ok(metadata) = std::fs::metadata(&path) else {
            return false;
        };

        metadata.is_file()
    }

    pub fn get_hachimi_installed_target(&self) -> Option<Target> {
        for target in Target::VALUES {
            if let Some(version_info) = self.get_target_version_info(*target) {
                if version_info.is_hachimi() {
                    return Some(*target);
                }
            }
        }
        None
    }

    pub fn install(&self) -> Result<(), Error> {
        // Verify install dir first
        let dest_dll = self.get_dest_dll_path().ok_or(Error::NoInstallDir)?;
        let src_dll = self.get_src_dll_path().ok_or(Error::NoInstallDir)?;

        if !dest_dll.exists() && !src_dll.exists() {
            return Err(Error::CannotFindTarget);
        }

        let path = self.get_current_target_path();
        let mut file = File::create(&path)?;

        #[cfg(feature = "compress_dll")]
        file.write(&include_bytes_zstd!("hachimi.dll", 19))?;

        #[cfg(not(feature = "compress_dll"))]
        file.write(include_bytes!("../hachimi.dll"))?;

        if src_dll.exists() {
            std::fs::create_dir_all(dest_dll.parent().unwrap())?;
            std::fs::copy(&src_dll, &dest_dll)?;
            std::fs::remove_file(&src_dll)?;
        }

        Ok(())
    }

    pub fn uninstall(&self) -> Result<(), Error> {
        let dest_dll = self.get_dest_dll_path().ok_or(Error::NoInstallDir)?;
        let src_dll = self.get_src_dll_path().ok_or(Error::NoInstallDir)?;

        let path = self.get_current_target_path();
        std::fs::remove_file(&path)?;

        if !src_dll.exists() {
            std::fs::copy(&dest_dll, &src_dll)?;
            std::fs::remove_file(&dest_dll)?;
        }

        Ok(())
    }
}

impl Default for Installer {
    fn default() -> Installer {
        Installer {
            install_dir: Self::detect_install_dir(),
            target: Target::default(),
            custom_target: None,
            system_dir: get_system_directory()
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Target {
    CriManaVpx
}

impl Target {
    pub const VALUES: &[Self] = &[Self::CriManaVpx];

    pub fn dll_name(&self) -> &'static str {
        match self {
            Self::CriManaVpx => "cri_mana_vpx.dll"
        }
    }
}

impl Default for Target {
    fn default() -> Self {
        Self::CriManaVpx
    }
}

#[derive(Debug, Default)]
pub struct TargetVersionInfo {
    pub name: Option<String>,
    pub version: Option<String>
}

impl TargetVersionInfo {
    pub fn get_display_label(&self, target: Target) -> String {
        let name = self.name.clone().unwrap_or_else(|| "Unknown".to_string());
        format!("* {} ({})", target.dll_name(), name)
    }

    pub fn is_hachimi(&self) -> bool {
        if let Some(name) = &self.name {
            return name == "Hachimi";
        }
        false
    }
}

#[derive(Debug)]
pub enum Error {
    NoInstallDir,
    CannotFindTarget,
    IoError(std::io::Error)
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoInstallDir => write!(f, "No install location specified"),
            Error::CannotFindTarget => write!(f, "Cannot find target DLL in specified install location"),
            Error::IoError(error) => write!(f, "I/O error: {}", error)
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IoError(e)
    }
}