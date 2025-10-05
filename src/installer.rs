use std::{fs::File, io::Write, path::{Path, PathBuf}};

use bsdiff::patch;
use pelite::resources::version_info::Language;
use registry::Hive;
use sha2::{Digest, Sha256};
use tinyjson::JsonValue;
use windows::{core::{w, HSTRING}, Win32::{Foundation::HWND, UI::{Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT}, WindowsAndMessaging::{MessageBoxW, IDOK, IDYES, MB_ICONINFORMATION, MB_ICONWARNING, MB_ICONQUESTION, MB_OK, MB_OKCANCEL, MB_YESNO}}}};

use crate::utils::{self, get_system_directory};

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum GameVersion {
    DMM,
    Steam
}

pub struct Installer {
    pub install_dir: Option<PathBuf>,
    pub game_version: Option<GameVersion>,
    pub target: Target,
    pub custom_target: Option<String>,
    system_dir: PathBuf,
    pub hwnd: Option<HWND>
}

impl Installer {
    pub fn custom(install_dir: Option<PathBuf>, target: Target, custom_target: Option<String>) -> Installer {
        let (resolved_install_dir, game_version) =
            if let Some(dir) = install_dir {
                if dir.join("umamusume.exe").is_file() {
                    (Some(dir), Some(GameVersion::DMM))
                } else if dir.join("UmamusumePrettyDerby_Jpn.exe").is_file() {
                    (Some(dir), Some(GameVersion::Steam))
                } else {
                    (Some(dir), None)
                }
            } else {
                if let Some(dmm_dir) = Self::detect_dmm_install_dir() {
                    (Some(dmm_dir), Some(GameVersion::DMM))
                } else if let Some(steam_dir) = Self::detect_steam_install_dir() {
                    (Some(steam_dir), Some(GameVersion::Steam))
                } else {
                    (None, None)
                }
            };

        Installer {
            install_dir: resolved_install_dir,
            game_version,
            target,
            custom_target,
            system_dir: get_system_directory(),
            hwnd: None
        }
    }

    fn detect_dmm_install_dir() -> Option<PathBuf> {
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

    fn detect_steam_install_dir() -> Option<PathBuf> {
        const STEAM_APP_ID: &str = "3564400";
        const GAME_FOLDER_NAME: &str = "UmamusumePrettyDerby_Jpn";
        const GAME_EXE_NAME: &str = "UmamusumePrettyDerby_Jpn.exe";

        let mut potential_libraries: Vec<PathBuf> = Vec::new();

        let default_steam_path = PathBuf::from(r"C:\Program Files (x86)\Steam");
        if default_steam_path.join("steam.exe").is_file() {
            potential_libraries.push(default_steam_path);
        }

        for letter in 'A'..='Z' {
            let drive_root = PathBuf::from(format!(r"{}:\", letter));

            let steam_library_path = drive_root.join("SteamLibrary");
            if steam_library_path.join("steam.dll").is_file() {
                potential_libraries.push(steam_library_path);
                continue;
            }

            if drive_root.join("steam.dll").is_file() {
                potential_libraries.push(drive_root);
            }
        }

        let manifest_filename = format!("appmanifest_{}.acf", STEAM_APP_ID);

        for library in potential_libraries {
            let manifest_path = library.join("steamapps").join(&manifest_filename);

            if manifest_path.is_file() {
                let game_path = library.join("steamapps\\common").join(GAME_FOLDER_NAME);

                if game_path.join(GAME_EXE_NAME).is_file() {
                    return Some(game_path);
                }
            }
        }

        None
    }

    fn get_install_method(&self, target: Target) -> InstallMethod {
        match target {
            Target::UnityPlayer => InstallMethod::DotLocal,
            Target::CriManaVpx => {
                if self.game_version == Some(GameVersion::Steam) {
                    InstallMethod::Direct
                } else {
                    InstallMethod::PluginShim
                }
            }
        }
    }

    fn get_target_path_internal(&self, target: Target, p: impl AsRef<Path>) -> Option<PathBuf> {
        let install_dir = self.install_dir.as_ref()?;
        Some(match self.get_install_method(target) {
            InstallMethod::DotLocal => install_dir.join("umamusume.exe.local").join(p),
            InstallMethod::PluginShim => self.system_dir.join(p),
            InstallMethod::Direct => install_dir.join(p),
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
        if self.get_install_method(self.target) == InstallMethod::PluginShim {
            let dest_dll = self.get_dest_plugin_path().ok_or(Error::NoInstallDir)?;
            let src_dll = self.get_src_plugin_path().ok_or(Error::NoInstallDir)?;

            if !dest_dll.exists() && !src_dll.exists() {
                return Err(Error::CannotFindTarget);
            }
        }

        Ok(())
    }

    pub fn install(&self) -> Result<(), Error> {
        let initial_dll_path = if self.target == Target::CriManaVpx && self.game_version == Some(GameVersion::Steam) {
            let install_dir = self.install_dir.as_ref().ok_or(Error::NoInstallDir)?;
            install_dir.join("hachimi").join(self.target.dll_name())
        } else {
            self.get_current_target_path().ok_or(Error::NoInstallDir)?
        };

        std::fs::create_dir_all(initial_dll_path.parent().unwrap())?;
        let mut file = File::create(&initial_dll_path)?;

        #[cfg(feature = "compress_dll")]
        file.write(&include_bytes_zstd!("hachimi.dll", 19))?;

        #[cfg(not(feature = "compress_dll"))]
        file.write(include_bytes!("../hachimi.dll"))?;

        let install_path = self.install_dir.as_ref().ok_or(Error::NoInstallDir)?;

        let steam_exe_path = install_path.join("UmamusumePrettyDerby_Jpn.exe");
        let dmm_exe_path = install_path.join("umamusume.exe");

        const EXPECTED_ORIGINAL_HASH: &str = "2173ea1e399a00b680ecfffc5b297ed1c29065f256a2f8b91ebcb66bc6315eb0";
        const NOPATCH_HASH: &str = "d578a228248ed61792a966c89089b7690a5ec403a89f4630a2aa0fa75ac9efec";

        if dmm_exe_path.is_file() {
            let exe_data = std::fs::read(&dmm_exe_path)?;
            let mut hasher = Sha256::new();
            hasher.update(&exe_data);
            let found_hash = hasher.finalize();
            let found_hash_str = format!("{:x}", found_hash);

            if found_hash_str.to_lowercase() == NOPATCH_HASH.to_lowercase() {} else {
                return Err(Error::VerificationError(format!(
                    "Found umamusume.exe, but its hash is incorrect. Expected {}, but found {}",
                    NOPATCH_HASH, found_hash_str
                )));
            }
        } else if steam_exe_path.is_file() {
            let original_exe_data = std::fs::read(&steam_exe_path)?;
            let mut hasher = Sha256::new();
            hasher.update(&original_exe_data);
            let found_hash = hasher.finalize();
            let found_hash_str = format!("{:x}", found_hash);

            if found_hash_str.to_lowercase() == EXPECTED_ORIGINAL_HASH.to_lowercase() {
                let patch_data = include_bytes!("../umamusume.patch");
                let temp_exe_path = steam_exe_path.with_extension("exe.tmp");
                let mut temp_exe_file = File::create(&temp_exe_path)?;

                let mut new_exe_data = Vec::new();
                patch(&original_exe_data, &mut std::io::Cursor::new(patch_data), &mut new_exe_data)?;

                temp_exe_file.write_all(&new_exe_data)?;

                std::fs::remove_file(&steam_exe_path)?;
                std::fs::rename(&temp_exe_path, &steam_exe_path)?;
            } else {
                return Err(Error::VerificationError(format!(
                    "Found UmamusumePrettyDerby_Jpn.exe, but its hash is incorrect. Expected {}, but found {}",
                    EXPECTED_ORIGINAL_HASH, found_hash_str
                )));
            }
        } else {
            return Err(Error::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not find UmamusumePrettyDerby_Jpn.exe or umamusume.exe."
            )));
        }

        if let Some(install_dir) = &self.install_dir {
            if let Some(steamapps_path) = find_steamapps_folder(install_dir) {
                const STEAM_APP_ID: &str = "3564400";
                let manifest_path = steamapps_path.join(format!("appmanifest_{}.acf", STEAM_APP_ID));
                let backup_path = manifest_path.with_extension("acf.bak");

                if manifest_path.is_file() {
                    if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                        if !content.contains("\"AutoUpdateBehavior\"\t\t\"1\"") {
                             let res = unsafe {
                                MessageBoxW(
                                    self.hwnd.as_ref(),
                                    w!("To prevent accidental updates that could break the mod, would you like to change Steam's auto-update setting for this game to 'Update only when I launch it'?\n\nA backup of your original setting will be made."),
                                    w!("Change Auto-Update Setting?"),
                                    MB_ICONQUESTION | MB_YESNO
                                )
                            };

                            if res == IDYES {
                                if !backup_path.exists() {
                                    std::fs::copy(&manifest_path, &backup_path)?;
                                }

                                let new_content = if content.contains("\"AutoUpdateBehavior\"\t\t\"0\"") {
                                    content.replace("\"AutoUpdateBehavior\"\t\t\"0\"", "\"AutoUpdateBehavior\"\t\t\"1\"")
                                } else if content.contains("\"AutoUpdateBehavior\"\t\t\"2\"") {
                                    content.replace("\"AutoUpdateBehavior\"\t\t\"2\"", "\"AutoUpdateBehavior\"\t\t\"1\"")
                                } else {
                                    content
                                };

                                if std::fs::write(&manifest_path, new_content).is_ok() {
                                    unsafe {
                                        MessageBoxW(
                                            self.hwnd.as_ref(),
                                            w!("Steam's auto-update setting for this game has been changed."),
                                            w!("Auto-update Setting Changed"),
                                            MB_ICONINFORMATION | MB_OK
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn post_install(&self) -> Result<(), Error> {
        if self.target == Target::CriManaVpx && self.game_version == Some(GameVersion::Steam) {
            let install_dir = self.install_dir.as_ref().ok_or(Error::NoInstallDir)?;
            let temp_path = install_dir.join("hachimi").join(self.target.dll_name());
            let final_path = self.get_current_target_path().ok_or(Error::NoInstallDir)?;

            if temp_path.is_file() {
                std::fs::rename(&temp_path, &final_path)?;
            }
        }

        match self.get_install_method(self.target) {
            InstallMethod::DotLocal => {
                // Install Cellar
                let path = self.install_dir.as_ref()
                    .ok_or_else(|| Error::NoInstallDir)?
                    .join("umamusume.exe.local")
                    .join("apphelp.dll");
                std::fs::create_dir_all(path.parent().unwrap())?;
                let mut file = File::create(&path)?;

                #[cfg(feature = "compress_dll")]
                file.write(&include_bytes_zstd!("cellar.dll", 19))?;

                #[cfg(not(feature = "compress_dll"))]
                file.write(include_bytes!("../cellar.dll"))?;

                // Check for DLL redirection
                match Hive::LocalMachine.open(
                    r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options",
                    registry::Security::Read | registry::Security::SetValue
                ) {
                    Ok(regkey) => {
                        if regkey.value("DevOverrideEnable")
                            .ok()
                            .map(|v| match v {
                                registry::Data::U32(v) => v,
                                _ => 0
                            })
                            .unwrap_or(0) == 0
                        {
                            let res = unsafe {
                                MessageBoxW(
                                    self.hwnd.as_ref(),
                                    w!("DotLocal DLL redirection is not enabled. This is required for the specified install target.\n\
                                        Would you like to enable it?"),
                                    w!("Install"),
                                    MB_ICONINFORMATION | MB_OKCANCEL
                                )
                            };
                            if res == IDOK {
                                regkey.set_value("DevOverrideEnable", &registry::Data::U32(1))?;
                                unsafe {
                                    MessageBoxW(
                                        self.hwnd.as_ref(),
                                        w!("Restart your computer to apply the changes."),
                                        w!("DLL redirection enabled"),
                                        MB_ICONINFORMATION | MB_OK
                                    );
                                }
                            }
                        }
                    },
                    Err(e) => {
                        unsafe { MessageBoxW(
                            self.hwnd.as_ref(),
                            &HSTRING::from(format!("Failed to open IFEO registry key: {}", e)),
                            w!("Warning"),
                            MB_OK | MB_ICONWARNING
                        )};
                    }
                }
            },
            InstallMethod::PluginShim => {
                let dest_dll = self.get_dest_plugin_path().ok_or(Error::NoInstallDir)?;
                let src_dll = self.get_src_plugin_path().ok_or(Error::NoInstallDir)?;

                if src_dll.exists() {
                    std::fs::create_dir_all(dest_dll.parent().unwrap())?;
                    std::fs::copy(&src_dll, &dest_dll)?;
                    std::fs::remove_file(&src_dll)?;
                }
            },
            InstallMethod::Direct => {}
        }

        Ok(())
    }

    pub fn uninstall(&self) -> Result<(), Error> {
        let path = self.get_current_target_path().ok_or(Error::NoInstallDir)?;
        std::fs::remove_file(&path)?;

        match self.get_install_method(self.target) {
            InstallMethod::DotLocal => {
                let parent = path.parent().unwrap();

                // Also delete Cellar
                _ = std::fs::remove_file(parent.join("apphelp.dll"));

                // Only remove if its empty
                _ = std::fs::remove_dir(parent);
            },
            InstallMethod::PluginShim => {
                let dest_dll = self.get_dest_plugin_path().ok_or(Error::NoInstallDir)?;
                let src_dll = self.get_src_plugin_path().ok_or(Error::NoInstallDir)?;
                if !src_dll.exists() {
                    std::fs::copy(&dest_dll, &src_dll)?;
                    std::fs::remove_file(&dest_dll)?;
                }
            },
            InstallMethod::Direct => {}
        }

        let install_path = self.install_dir.as_ref().ok_or(Error::NoInstallDir)?;
        let exe_path = install_path.join("UmamusumePrettyDerby_Jpn.exe");

        if exe_path.is_file() {
            let reverse_patch_data = include_bytes!("../umamusume.rev.patch");
            let patched_exe_data = std::fs::read(&exe_path)?;

            const EXPECTED_PATCHED_HASH: &str = "9d6955463a0a509a2355d2227a4ee9ef0ca5da3f0f908b0c846a1e3c218cb703";

            let mut hasher = Sha256::new();
            hasher.update(&patched_exe_data);
            let found_hash = hasher.finalize();
            let found_hash_str = format!("{:x}", found_hash);

            if found_hash_str.to_lowercase() == EXPECTED_PATCHED_HASH.to_lowercase() {
                let mut original_exe_data = Vec::new();

                patch(&patched_exe_data, &mut std::io::Cursor::new(reverse_patch_data), &mut original_exe_data)?;

                let temp_exe_path = exe_path.with_extension("exe.tmp");
                std::fs::write(&temp_exe_path, &original_exe_data)?;
                std::fs::rename(&temp_exe_path, &exe_path)?;
            }
        }

        if let Some(install_dir) = &self.install_dir {
            if let Some(steamapps_path) = find_steamapps_folder(install_dir) {
                const STEAM_APP_ID: &str = "3564400";
                let manifest_path = steamapps_path.join(format!("appmanifest_{}.acf", STEAM_APP_ID));
                let backup_path = manifest_path.with_extension("acf.bak");

                if backup_path.is_file() {
                    let res = unsafe {
                        MessageBoxW(
                            self.hwnd.as_ref(),
                            w!("Would you like to restore your original Steam auto-update setting for this game?"),
                            w!("Restore Auto-Update Setting?"),
                            MB_ICONQUESTION | MB_YESNO
                        )
                    };

                    if res == IDYES {
                        if let (Ok(live_content), Ok(backup_content)) = (std::fs::read_to_string(&manifest_path), std::fs::read_to_string(&backup_path)) {
                            let original_setting = backup_content.lines().find(|l| l.contains("\"AutoUpdateBehavior\""));
                            let current_setting = live_content.lines().find(|l| l.contains("\"AutoUpdateBehavior\""));

                            if let (Some(original), Some(current)) = (original_setting, current_setting) {
                                let new_content = live_content.replace(current, original);
                                if std::fs::write(&manifest_path, new_content).is_ok() {
                                    _ = std::fs::remove_file(&backup_path);

                                    unsafe {
                                        MessageBoxW(
                                            self.hwnd.as_ref(),
                                            w!("Your original auto-update setting has been restored."),
                                            w!("Setting Restored"),
                                            MB_ICONINFORMATION | MB_OK
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn get_dest_plugin_path(&self) -> Option<PathBuf> {
        Some(self.install_dir.as_ref()?.join(format!("hachimi\\{}", self.target.dll_name())))
    }

    pub fn get_src_plugin_path(&self) -> Option<PathBuf> {
        Some(self.install_dir.as_ref()?.join(format!("umamusume_Data\\Plugins\\x86_64\\{}", self.target.dll_name())))
    }
}

fn find_steamapps_folder(game_install_dir: &Path) -> Option<PathBuf> {
    let mut current = game_install_dir.to_path_buf();
    while let Some(parent) = current.parent() {
        let steamapps_path = parent.join("steamapps");
        if steamapps_path.is_dir() {
            return Some(steamapps_path);
        }
        current = parent.to_path_buf();
        if current.parent().is_none() {
            break;
        }
    }
    None
}

impl Default for Installer {
    fn default() -> Installer {
        let (install_dir, game_version) = 
            if let Some(dmm_dir) = Self::detect_dmm_install_dir() {
                (Some(dmm_dir), Some(GameVersion::DMM))
            } else if let Some(steam_dir) = Self::detect_steam_install_dir() {
                (Some(steam_dir), Some(GameVersion::Steam))
            } else {
                (None, None)
            };

        Installer {
            install_dir,
            game_version,
            target: Target::default(),
            custom_target: None,
            system_dir: get_system_directory(),
            hwnd: None
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Target {
    UnityPlayer,
    CriManaVpx
}

impl Target {
    pub const VALUES: &[Self] = &[
        Self::UnityPlayer,
        Self::CriManaVpx
    ];

    pub fn dll_name(&self) -> &'static str {
        match self {
            Self::UnityPlayer => "UnityPlayer.dll",
            Self::CriManaVpx => "cri_mana_vpx.dll"
        }
    }
}

impl Default for Target {
    fn default() -> Self {
        Self::UnityPlayer
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum InstallMethod {
    DotLocal,
    PluginShim,
    Direct,
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
    IoError(std::io::Error),
    RegistryValueError(registry::value::Error),
    VerificationError(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoInstallDir => write!(f, "No install location specified"),
            Error::CannotFindTarget => write!(f, "Cannot find target DLL in specified install location"),
            Error::IoError(e) => write!(f, "I/O error: {}", e),
            Error::RegistryValueError(e) => write!(f, "Registry value error: {}", e),
            Error::VerificationError(e) => write!(f, "Verification error: {}", e),
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