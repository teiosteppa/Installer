use std::{fs::File, io::{Write, Read}, path::{Path, PathBuf}};

use pelite::resources::version_info::Language;
use registry::Hive;
use steamlocate::SteamDir;
use tinyjson::JsonValue;
use windows::{core::{w, HSTRING}, Win32::{Foundation::HWND, UI::{Shell::{FOLDERID_RoamingAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT}, WindowsAndMessaging::{MessageBoxW, IDOK, IDYES, MB_ICONINFORMATION, MB_ICONWARNING, MB_ICONQUESTION, MB_OK, MB_OKCANCEL, MB_YESNO}}}};

use crate::utils::{self, get_system_directory};

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum GameVersion {
    DMM,
    Steam
}

pub struct Installer {
    dmm_install_dir: Option<PathBuf>,
    steam_install_dir: Option<PathBuf>,
    install_dir: Option<PathBuf>,
    game_version: Option<GameVersion>,

    pub target: Target,
    pub custom_target: Option<String>,
    system_dir: PathBuf,
    pub hwnd: Option<HWND>
}

impl Installer {
    fn detect_version_from_dir(dir: &Path) -> Option<GameVersion> {
        if dir.join("umamusume.exe").is_file() {
            Some(GameVersion::DMM)
        } else if dir.join("UmamusumePrettyDerby_Jpn.exe").is_file() {
            Some(GameVersion::Steam)
        } else {
            None
        }
    }

    pub fn new(target: Target, custom_target: Option<String>) -> Installer {
        Installer {
            dmm_install_dir: None,
            steam_install_dir: None,
            install_dir: None,
            game_version: None,
            target,
            custom_target,
            system_dir: get_system_directory(),
            hwnd: None
        }
    }

    pub fn set_install_dir(&mut self, dir: PathBuf) -> Result<(), Error> {
        match Self::detect_version_from_dir(&dir) {
            Some(version) => {
                self.install_dir = Some(dir.clone());
                self.game_version = Some(version);
                match version {
                    GameVersion::DMM => self.dmm_install_dir = Some(dir),
                    GameVersion::Steam => self.steam_install_dir = Some(dir),
                }
                Ok(())
            }
            None => Err(Error::InvalidInstallDir)
        }
    }

    pub fn install_dir(&self) -> Option<&PathBuf> {
        self.install_dir.as_ref()
    }

    pub fn game_version(&self) -> Option<GameVersion> {
        self.game_version
    }

    pub fn detect_install_dir(&mut self) {
        if let Some(dmm_dir) = Self::detect_dmm_install_dir() {
            self.install_dir = Some(dmm_dir);
            self.game_version = Some(GameVersion::DMM);
        } else if let Some(steam_dir) = Self::detect_steam_install_dir() {
            self.install_dir = Some(steam_dir);
            self.game_version = Some(GameVersion::Steam);
        }
    }

    pub fn detect_install_dirs(&mut self) {
        self.dmm_install_dir = Self::detect_dmm_install_dir();
        self.steam_install_dir = Self::detect_steam_install_dir();

        if self.install_dir.is_none() {
            if self.dmm_install_dir.is_some() {
                self.set_game_version(GameVersion::DMM);
            } else if self.steam_install_dir.is_some() {
                self.set_game_version(GameVersion::Steam);
            }
        }
    }

    pub fn dmm_install_dir(&self) -> Option<&PathBuf> {
        self.dmm_install_dir.as_ref()
    }

    pub fn steam_install_dir(&self) -> Option<&PathBuf> {
        self.steam_install_dir.as_ref()
    }

    pub fn set_game_version(&mut self, version: GameVersion) -> Option<&PathBuf> {
        self.game_version = Some(version);
        match version {
            GameVersion::DMM => self.install_dir = self.dmm_install_dir.clone(),
            GameVersion::Steam => self.install_dir = self.steam_install_dir.clone(),
        }
        self.install_dir.as_ref()
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
        const STEAM_APP_ID: u32 = 3564400;
        const GAME_EXE_NAME: &str = "UmamusumePrettyDerby_Jpn.exe";

        if let Ok(steamdir) = SteamDir::locate() {
            if let Ok(Some((app, library))) = steamdir.find_app(STEAM_APP_ID) {

                let game_path = library.path()
                    .join("steamapps")
                    .join("common")
                    .join(&app.install_dir);

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
            InstallMethod::DotLocal => {
                let exe_name = match self.game_version {
                    Some(GameVersion::Steam) => "UmamusumePrettyDerby_Jpn.exe",
                    _ => "umamusume.exe",
                };
                let local_folder_name = format!("{}.local", exe_name);
                install_dir.join(local_folder_name).join(p)
            }
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
        let initial_dll_path = self.get_current_target_path().ok_or(Error::NoInstallDir)?;

        std::fs::create_dir_all(initial_dll_path.parent().unwrap())?;
        let mut file = File::create(&initial_dll_path)?;

        #[cfg(feature = "compress_dll")]
        file.write(&include_bytes_zstd!("hachimi.dll", 19))?;

        #[cfg(not(feature = "compress_dll"))]
        file.write(include_bytes!("../hachimi.dll"))?;

        let install_path = self.install_dir.as_ref().ok_or(Error::NoInstallDir)?;

        const EXPECTED_ORIGINAL_HASH: &str = "47e89b30dcd44219c9d7eb4dca0a721c694f7887ebdb1905f92f1a54841a074b";

        match self.game_version {
            Some(GameVersion::DMM) => {},
            Some(GameVersion::Steam) => {
                let steam_exe_path = install_path.join("UmamusumePrettyDerby_Jpn.exe");
                let backup_exe_path = steam_exe_path.with_extension("exe.bak");

                if let Err(e) = utils::verify_file_hash(&steam_exe_path, EXPECTED_ORIGINAL_HASH) {
                    return Err(Error::VerificationError(format!("Found UmamusumePrettyDerby_Jpn.exe, but its hash is incorrect. {}", e)));
                }

                if !backup_exe_path.exists() {
                    std::fs::copy(&steam_exe_path, &backup_exe_path)?;
                }

                let original_exe_data = std::fs::read(&steam_exe_path)?;
                let compressed_patch_data = include_bytes!("../umamusume.patch.zst");
                let mut patch_data = Vec::new();
                let mut decoder = zstd::Decoder::new(&compressed_patch_data[..])?;
                decoder.read_to_end(&mut patch_data)?;

                let temp_exe_path = steam_exe_path.with_extension("exe.tmp");
                
                utils::apply_patch(&original_exe_data, &patch_data, &temp_exe_path)
                    .map_err(|e| Error::Generic(e.to_string().into()))?;

                std::fs::remove_file(&steam_exe_path)?;
                std::fs::rename(&temp_exe_path, &steam_exe_path)?;
            },
            None => {
                return Err(Error::IoError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not find a valid game executable."
                )));
            }
        }

        if let Some(GameVersion::Steam) = self.game_version &&
           let Some(install_dir) = &self.install_dir &&
           let Some(steamapps_path) = find_steamapps_folder(install_dir)
        {
            const STEAM_APP_ID: &str = "3564400";
            let manifest_path = steamapps_path.join(format!("appmanifest_{}.acf", STEAM_APP_ID));
            let backup_path = manifest_path.with_extension("acf.bak");

            if !manifest_path.is_file() {
                return Ok(());
            }
            
            let Ok(content) = std::fs::read_to_string(&manifest_path) else { return Ok(()) };

            if content.contains("\"AutoUpdateBehavior\"\t\t\"1\"") {
                return Ok(());
            }
            
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
                let new_content = content.replace("\"AutoUpdateBehavior\"\t\t\"0\"", "\"AutoUpdateBehavior\"\t\t\"1\"");
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

        Ok(())
    }

    pub fn post_install(&self) -> Result<(), Error> {
        match self.get_install_method(self.target) {
            InstallMethod::DotLocal => {
                // Install Cellar
                let main_dll_path = self.get_current_target_path().ok_or(Error::NoInstallDir)?;
                let parent_dir = main_dll_path.parent().unwrap();

                let path = parent_dir.join("apphelp.dll");
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
        let backup_path = exe_path.with_extension("exe.bak");

        if backup_path.is_file() {
            std::fs::remove_file(&exe_path)?;
            std::fs::rename(&backup_path, &exe_path)?;
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
        let mut installer = Self::new(Target::default(), None);
        installer.detect_install_dirs();
        installer
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
    InvalidInstallDir,
    CannotFindTarget,
    IoError(std::io::Error),
    RegistryValueError(registry::value::Error),
    VerificationError(String),
    Generic(Box<dyn std::error::Error + Send + Sync>),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoInstallDir => write!(f, "No install location specified"),
            Error::InvalidInstallDir => write!(f, "Invalid game folder. The selected folder does not contain umamusume.exe or UmamusumePrettyDerby_Jpn.exe."),
            Error::CannotFindTarget => write!(f, "Cannot find target DLL in specified install location"),
            Error::IoError(e) => write!(f, "I/O error: {}", e),
            Error::RegistryValueError(e) => write!(f, "Registry value error: {}", e),
            Error::VerificationError(e) => write!(f, "Verification error: {}", e),
            Error::Generic(e) => write!(f, "An unexpected error occurred: {}", e),
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