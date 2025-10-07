use std::{fs::File, io::{Read, Write}, path::{Path, PathBuf}, };

use pelite::resources::version_info::Language;
// use registry::Hive;
// use tinyjson::JsonValue;
// use windows::{core::{w, HSTRING}, Win32::{Foundation::HWND, UI::{Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT}, WindowsAndMessaging::{MessageBoxW, IDOK, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_OKCANCEL}}}};
use windows::{Win32::{Foundation::HWND}};
use steamlocate::SteamDir;
use bsdiff;
use crate::utils::{self};

pub struct Installer {
    pub install_dir: Option<PathBuf>,
    pub target: Target,
    pub custom_target: Option<String>,
    pub hwnd: Option<HWND>
}

impl Installer {
    pub fn custom(install_dir: Option<PathBuf>, target: Target, custom_target: Option<String>) -> Installer {
        Installer {
            install_dir: install_dir.or_else(Self::detect_install_dir),
            target,
            custom_target,
            hwnd: None
        }
    }

    // todo: allow both dmm and steam to be installed with one exe
    // original detect_install_dir:
    //
    // fn detect_dmm_install_dir() -> Option<PathBuf> {
    //     let app_data_dir_wstr = unsafe { SHGetKnownFolderPath(&FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, None).ok()? };
    //     let app_data_dir_str = unsafe { app_data_dir_wstr.to_string().ok()? };
    //     let app_data_dir = Path::new(&app_data_dir_str);
    //     let mut dmm_config_path = app_data_dir.join("dmmgameplayer5");
    //     dmm_config_path.push("dmmgame.cnf");

    //     let config_str = std::fs::read_to_string(dmm_config_path).ok()?;
    //     let JsonValue::Object(config) = config_str.parse().ok()? else {
    //         return None;
    //     };
    //     let JsonValue::Array(config_contents) = &config["contents"] else {
    //         return None;
    //     };
    //     for value in config_contents {
    //         let JsonValue::Object(game) = value else {
    //             return None;
    //         };

    //         let JsonValue::String(product_id) = &game["productId"] else {
    //             continue;
    //         };
    //         if product_id != "umamusume" {
    //             continue;
    //         }

    //         let JsonValue::Object(detail) = &game["detail"] else {
    //             return None;
    //         };
    //         let JsonValue::String(path_str) = &detail["path"] else {
    //             return None;
    //         };

    //         let path = PathBuf::from(path_str);
    //         return if path.is_dir() {
    //             Some(path)
    //         }
    //         else {
    //             None
    //         }
    //     }

    //     None
    // }

    fn detect_steam_install_dir() -> Option<PathBuf> {
        let steam_dir = SteamDir::locate().ok()?;
        let (uma_musume_steamapp, _lib) = steam_dir
            .find_app(3564400)
            .ok()??;
        let game_path = _lib.resolve_app_dir(&uma_musume_steamapp);
        if game_path.is_dir() { return Some(game_path) };
        None
    }

    fn detect_install_dir() -> Option<PathBuf> {
        // lazy since this is a fork, just check for steam first & fallback to DMM (unimplemented)
        if let Some(path) = Self::detect_steam_install_dir() {
            return Some(path);
        }
        // if let Some(path) = Self::detect_dmm_install_dir() {
        //     return Some(path);
        // }

        None
    }

    //something exe something something
    fn get_target_path_internal(&self, target: Target, p: impl AsRef<Path>) -> Option<PathBuf> {
        Some(match TargetType::from(target) {
            // TargetType::DotLocal => self.install_dir.as_ref()?.join("UmamusumePrettyDerby_Jpn.exe.local").join(p),
            TargetType::PluginShim => self.install_dir.as_ref()?.join(p)
        })
    }

    pub fn get_target_path(&self, target: Target) -> Option<PathBuf> {
        self.get_target_path_internal(target, target.dll_name())
    }

    pub fn get_current_target_path(&self) -> Option<PathBuf> {
        self.get_target_path_internal(self.target, if let Some(custom_target) = &self.custom_target {
            custom_target
        }
        else {
            self.target.dll_name()
        })
    }

    const LANG_NEUTRAL_UNICODE: Language = Language { lang_id: 0x0000, charset_id: 0x04b0 };
    pub fn get_target_version_info(&self, target: Target) -> Option<TargetVersionInfo> {
        let path = self.get_target_path(target)?;
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
        let Some(path) = self.get_current_target_path() else {
            return false;
        };

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

    pub fn pre_install(&self) -> Result<(), Error> {
        if TargetType::from(self.target) == TargetType::PluginShim {
            //something exe idk
            let orig_exe = self.get_orig_exe_path().ok_or(Error::NoInstallDir)?;
            let backup_exe = self.get_backup_exe_path().ok_or(Error::NoInstallDir)?;

            if backup_exe.exists() {
                std::fs::remove_file(&backup_exe)?;
            }
            std::fs::copy(&orig_exe, &backup_exe)?;
        }

        Ok(())
    }

    pub fn install(&self) -> Result<(), Error> {
        let path = self.get_current_target_path().ok_or(Error::NoInstallDir)?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        let mut file = File::create(&path)?;

        #[cfg(feature = "compress_bin")]
        file.write(&include_bytes_zstd!("hachimi.dll", 19))?;

        #[cfg(not(feature = "compress_bin"))]
        file.write(include_bytes!("../hachimi.dll"))?;

        Ok(())
    }

    // no .local redirection necessary on steam client, so dropped that, wheee
    // greetz to uma on mac / linux
    pub fn post_install(&self) -> Result<(), Error> {
        match TargetType::from(self.target) {
            // TargetType::DotLocal => {
            //     // Install Cellar
            //     let path = self.install_dir.as_ref()
            //         .ok_or_else(|| Error::NoInstallDir)?
            //         .join("UmamusumePrettyDerby_Jpn.exe.local")
            //         .join("apphelp.dll");
            //     std::fs::create_dir_all(path.parent().unwrap())?;
            //     let mut file = File::create(&path)?;

            //     #[cfg(feature = "compress_bin")]
            //     file.write(&include_bytes_zstd!("cellar.dll", 19))?;

            //     #[cfg(not(feature = "compress_bin"))]
            //     file.write(include_bytes!("../cellar.dll"))?;

            //     // Check for DLL redirection
            //     match Hive::LocalMachine.open(
            //         r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options",
            //         registry::Security::Read | registry::Security::SetValue
            //     ) {
            //         Ok(regkey) => {
            //             if regkey.value("DevOverrideEnable")
            //                 .ok()
            //                 .map(|v| match v {
            //                     registry::Data::U32(v) => v,
            //                     _ => 0
            //                 })
            //                 .unwrap_or(0) == 0
            //             {
            //                 let res = unsafe {
            //                     MessageBoxW(
            //                         self.hwnd.as_ref(),
            //                         w!("DotLocal DLL redirection is not enabled. This is required for the specified install target.\n\
            //                             Would you like to enable it?"),
            //                         w!("Install"),
            //                         MB_ICONINFORMATION | MB_OKCANCEL
            //                     )
            //                 };
            //                 if res == IDOK {
            //                     regkey.set_value("DevOverrideEnable", &registry::Data::U32(1))?;
            //                     unsafe {
            //                         MessageBoxW(
            //                             self.hwnd.as_ref(),
            //                             w!("Restart your computer to apply the changes."),
            //                             w!("DLL redirection enabled"),
            //                             MB_ICONINFORMATION | MB_OK
            //                         );
            //                     }
            //                 }
            //             }
            //         },
            //         Err(e) => {
            //             unsafe { MessageBoxW(
            //                 self.hwnd.as_ref(),
            //                 &HSTRING::from(format!("Failed to open IFEO registry key: {}", e)),
            //                 w!("Warning"),
            //                 MB_OK | MB_ICONWARNING
            //             )};
            //         }
            //     }
            // },
            TargetType::PluginShim => {
                let exe_path = self.get_orig_exe_path().ok_or(Error::NoInstallDir)?;

                let exe_file = std::fs::read(&exe_path)?;
                let mut patch = Vec::new();

                #[cfg(feature = "compress_bin")]
                bsdiff::diff(&exe_file, &include_bytes_zstd!("FunnyHoney.exe", 19), &mut patch)?;
                #[cfg(not(feature = "compress_bin"))]
                bsdiff::diff(&exe_file, include_bytes!("../FunnyHoney.exe"), &mut patch)?;

                std::fs::write(&exe_path, &patch)?;
            }
        }

        Ok(())
    }

    pub fn uninstall(&self) -> Result<(), Error> {
        let path = self.get_current_target_path().ok_or(Error::NoInstallDir)?;
        std::fs::remove_file(&path)?;

        match TargetType::from(self.target) {
            // TargetType::DotLocal => {
            //     let parent = path.parent().unwrap();

            //     // Also delete Cellar
            //     _ = std::fs::remove_file(parent.join("apphelp.dll"));

            //     // Only remove if its empty
            //     _ = std::fs::remove_dir(parent);
            // },
            TargetType::PluginShim => {
                let backup_exe = self.get_backup_exe_path().ok_or(Error::NoInstallDir)?;
                let orig_exe = self.get_orig_exe_path().ok_or(Error::NoInstallDir)?;
                if backup_exe.exists() {
                    std::fs::copy(&backup_exe, &orig_exe)?;
                    std::fs::remove_file(&backup_exe)?;
                }
            }
        }

        Ok(())
    }

    pub fn get_backup_exe_path(&self) -> Option<PathBuf> {
        Some(self.install_dir.as_ref()?.join("UmamusumePrettyDerby_Jpn.old.exe"))
    }

    pub fn get_orig_exe_path(&self) -> Option<PathBuf> {
        Some(self.install_dir.as_ref()?.join("UmamusumePrettyDerby_Jpn.exe"))
    }
}

impl Default for Installer {
    fn default() -> Installer {
        Installer {
            install_dir: Self::detect_install_dir(),
            target: Target::default(),
            custom_target: None,
            hwnd: None
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Target {
    // UnityPlayer,
    CriManaVpx
}

impl Target {
    pub const VALUES: &[Self] = &[
        // Self::UnityPlayer,
        Self::CriManaVpx
    ];

    pub fn dll_name(&self) -> &'static str {
        match self {
            // Self::UnityPlayer => "UnityPlayer.dll",
            Self::CriManaVpx => "cri_mana_vpx.dll"
        }
    }
}

impl Default for Target {
    fn default() -> Self {
        // Self::UnityPlayer
        Self::CriManaVpx
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TargetType {
    // DotLocal,
    PluginShim
}

impl From<Target> for TargetType {
    fn from(value: Target) -> Self {
        match value {
            // Target::UnityPlayer => Self::DotLocal,
            Target::CriManaVpx => Self::PluginShim,
        }
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
    IoError(std::io::Error),
    RegistryValueError(registry::value::Error),
    // BackupNotFound
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoInstallDir => write!(f, "No install location specified"),
            Error::IoError(e) => write!(f, "I/O error: {}", e),
            Error::RegistryValueError(e) => write!(f, "Registry value error: {}", e),
            // Error::BackupNotFound => write!(f, "Failed to restore backup. Validate game integrity in Steam before launching.")
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IoError(e)
    }
}

impl From<registry::value::Error> for Error {
    fn from(e: registry::value::Error) -> Self {
        Error::RegistryValueError(e)
    }
}