use std::{fs::File, io::Write, path::{Path, PathBuf}};
#[cfg(feature = "net_install")]
use std::sync::{Arc, Mutex};
use pelite::resources::version_info::Language;
// use registry::Hive;
// use tinyjson::JsonValue;
// use windows::{core::{w, HSTRING}, Win32::{Foundation::HWND, UI::{Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT}, WindowsAndMessaging::{MessageBoxW, IDOK, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_OKCANCEL}}}};
use windows::{Win32::{Foundation::HWND}};
#[cfg(feature = "net_install")]
use bytes::Bytes;
use steamlocate::SteamDir;
use bsdiff;
use crate::utils::{self};

#[cfg(feature = "net_install")]
type DownloadResult = Result<Bytes, reqwest::Error>;

pub struct Installer {
    pub install_dir: Option<PathBuf>,
    pub target: Target,
    pub custom_target: Option<String>,
    pub hwnd: Option<HWND>,
    #[cfg(feature = "net_install")]
    pub hachimi_dll: Arc<Mutex<Option<DownloadResult>>>
}

impl Installer {
    pub fn custom(install_dir: Option<PathBuf>, target: Target, custom_target: Option<String>) -> Installer {
        Installer {
            install_dir: install_dir.or_else(Self::detect_install_dir),
            target,
            custom_target,
            hwnd: None,
            #[cfg(feature = "net_install")]
            hachimi_dll: Arc::new(Mutex::new(None))
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
            TargetType::Direct => self.install_dir.as_ref()?.join(p)
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
        if TargetType::from(self.target) == TargetType::Direct {
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

        let mod_dll: Vec<u8>;

        #[cfg(feature = "net_install")]
        {
            // The download is started in the background. Here we wait for it to complete.
            // A simple approach is to just lock and check.
            // For a better UX, you could show a progress indicator and not block the install button until download is ready.
            // This implementation will still block if the download isn't finished.
            // We use `take()` to move the value out of the Option, leaving None in its place.
            // This avoids the need to clone the `reqwest::Error`, which doesn't implement `Clone`.
            let guard = self.hachimi_dll.lock().unwrap();
            match guard.as_ref() {
                Some(Ok(bytes)) => {
                    // `Bytes` is cheap to clone (atomic reference count).
                    mod_dll = bytes.clone().into();
                }
                Some(Err(_)) => {
                    // The error can't be cloned. We return a generic error.
                    // A more robust solution could involve storing a cloneable error type.
                    return Err(Error::DownloadFailed);
                }
                None => {
                    return Err(Error::DownloadNotStarted);
                }
            }
        }
        #[cfg(not(feature = "net_install"))]
        {
            mod_dll = include_bytes!("../hachimi.dll").to_vec();
        }

        std::fs::create_dir_all(path.parent().unwrap())?;
        let mut file = File::create(&path)?;
        file.write_all(&mod_dll)?;

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
            TargetType::Direct => {
                let exe_path = self.get_orig_exe_path().ok_or(Error::NoInstallDir)?;

                // just use stdlib here cuz binary is so small
                let exe_bytes = std::fs::read(&exe_path)?;
                #[cfg(feature = "compress_bin")]
                let modded_bytes: &[u8] = &include_bytes_zstd!("FunnyHoney.exe", 19);
                #[cfg(not(feature = "compress_bin"))]
                let modded_bytes: &[u8] = include_bytes!("../FunnyHoney.exe");
                let mut patch = Vec::new(); {
                    bsdiff::diff(&exe_bytes, &modded_bytes, &mut patch)?;
                }

                let mut patched_bytes = Vec::with_capacity(modded_bytes.len()); {
                    bsdiff::patch(&exe_bytes, &mut patch.as_slice(), &mut patched_bytes)?;
                }
                debug_assert_eq!(modded_bytes, patched_bytes);

                // Write tmpfile before overwriting shim EXE
                // atomic replace so game dont break if patch fails
                let mut patched_exe = File::create(&exe_path.with_extension("exe.tmp"))?;
                patched_exe.write(&patched_bytes)?;
                std::fs::rename(&exe_path.with_extension("exe.tmp"), &exe_path)?;
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
            TargetType::Direct => {
                let backup_exe = self.get_backup_exe_path().ok_or(Error::NoInstallDir)?;
                let orig_exe = self.get_orig_exe_path().ok_or(Error::NoInstallDir)?;
                if backup_exe.exists() {
                    std::fs::rename(&backup_exe, &orig_exe)?;
                } else {
                    return Err(Error::FailedToRestore);
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
            hwnd: None,
            #[cfg(feature = "net_install")]
            hachimi_dll: Arc::new(Mutex::new(None))
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
    Direct
}

impl From<Target> for TargetType {
    fn from(value: Target) -> Self {
        match value {
            // Target::UnityPlayer => Self::DotLocal,
            Target::CriManaVpx => Self::Direct,
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
    FailedToRestore,
    #[cfg(feature = "net_install")]
    ReqwestError(reqwest::Error),
    #[cfg(feature = "net_install")]
    DownloadNotStarted,
    #[cfg(feature = "net_install")]
    DownloadFailed,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoInstallDir => write!(f, "No install location specified"),
            Error::IoError(e) => write!(f, "I/O error: {}", e),
            Error::RegistryValueError(e) => write!(f, "Registry value error: {}", e),
            Error::FailedToRestore => write!(f, "Failed to restore backup. Validate game integrity in Steam before launching."),
            #[cfg(feature = "net_install")]
            Error::ReqwestError(e) => write!(f, "Download error: {}", e),
            #[cfg(feature = "net_install")]
            Error::DownloadFailed => write!(f, "Download failed on a previous attempt. Please restart the installer."),
            #[cfg(feature = "net_install")]
            Error::DownloadNotStarted => write!(f, "Download has not started."),
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

#[cfg(feature = "net_install")]
impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::ReqwestError(e)
    }
}