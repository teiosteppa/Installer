use crate::{installer::{self, GameVersion, Installer}, resource::*, updater::UpdateStatus, utils};
use crate::i18n::{self, SUPPORTED_LOCALES, t};
use windows::{core::HSTRING, Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    System::{Com::{CoInitializeEx, COINIT_APARTMENTTHREADED}, LibraryLoader::GetModuleHandleW},
    UI::{Controls::{BST_CHECKED, BST_UNCHECKED}, Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::{
        BM_SETCHECK, CreateDialogParamW, DestroyIcon, DispatchMessageW, GetDlgItem, GetMessageW,
        GetWindowLongPtrW, IsDialogMessageW, LoadIconW, MessageBoxW, PostQuitMessage, SendMessageW,
        SetWindowLongPtrW,SetWindowTextW, ShowWindow, TranslateMessage,
        CBN_SELCHANGE, CB_ADDSTRING, CB_DELETESTRING, CB_GETCURSEL, CB_INSERTSTRING, CB_RESETCONTENT, CB_SETCURSEL,
        GWLP_USERDATA, ICON_BIG, IDOK, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING,
        MB_OK, MB_OKCANCEL, MB_YESNO, MSG, SW_SHOW, WM_CLOSE, WM_COMMAND, WM_INITDIALOG, WM_SETICON,
        SetTimer, KillTimer
    }}
}};

const ID_TIMER_GAMERUNNING: usize = 1;

fn localize_controls(dialog: HWND) {
    unsafe {
        // Title
        _ = SetWindowTextW(dialog, &HSTRING::from(t!("gui.title")));
        // Button / Tag
        _ = SetWindowTextW(GetDlgItem(dialog, IDC_INSTALL).unwrap(),   &HSTRING::from(t!("gui.install")));
        _ = SetWindowTextW(GetDlgItem(dialog, IDC_UNINSTALL).unwrap(), &HSTRING::from(t!("gui.uninstall")));
        _ = SetWindowTextW(GetDlgItem(dialog, IDC_INSTALL_PATH_BROWSE).unwrap(), &HSTRING::from(t!("gui.browse")));
        _ = SetWindowTextW(GetDlgItem(dialog, IDC_LANGUAGE_LABEL).unwrap(), &HSTRING::from(t!("gui.msg_language")));
        _ = SetWindowTextW(GetDlgItem(dialog, IDC_PACKAGED_VER).unwrap(), &HSTRING::from(t!("gui.packaged_ver", ver = env!("HACHIMI_VERSION"))));
        _ = SetWindowTextW(GetDlgItem(dialog, IDC_INSTALL_LOCATION).unwrap(), &HSTRING::from(t!("gui.install_location")));
        _ = SetWindowTextW(GetDlgItem(dialog, IDC_TARGRT).unwrap(), &HSTRING::from(t!("gui.target")));
        _ = SetWindowTextW(GetDlgItem(dialog, IDC_VERSION_GROUP).unwrap(), &HSTRING::from(t!("gui.game_version")));

        if let Ok(btn) = GetDlgItem(dialog, IDC_VERSION_STEAM) {
            _ = SetWindowTextW(btn, &HSTRING::from(t!("gui.steam_jp")));
        }
        if let Ok(btn) = GetDlgItem(dialog, IDC_VERSION_STEAM_GLOBAL) {
            _ = SetWindowTextW(btn, &HSTRING::from(t!("gui.steam_global")));
        }
    }
}

pub fn run(update_status: UpdateStatus) -> Result<(), windows::core::Error> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }

    match update_status {
        UpdateStatus::Updated(date) => {
            let message = t!("gui.update_success_body", date = date);
            unsafe {
                MessageBoxW(None, &HSTRING::from(message), &HSTRING::from(t!("gui.update_successful")), MB_ICONINFORMATION | MB_OK);
            }
            std::process::exit(0);
        }
        UpdateStatus::NotNeeded | UpdateStatus::Disabled => {}
        UpdateStatus::Failed(msg) => {
            let message = t!("gui.update_error_body", error = msg);
            unsafe {
                MessageBoxW(None, &HSTRING::from(message), &HSTRING::from(t!("gui.update_error")), MB_ICONERROR | MB_OK);
            }
        }
    }

    let mut installer = Box::new(Installer::default());

    let instance = unsafe { GetModuleHandleW(None)? };
    let dialog = unsafe {
        CreateDialogParamW(instance, IDD_MAIN, None, Some(dlg_proc), LPARAM(installer.as_mut() as *mut _ as _))
    }?;
    utils::center_window(dialog)?;
    let _ = unsafe { ShowWindow(dialog, SW_SHOW) };
    installer.hwnd = Some(dialog);

    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
        if !unsafe { IsDialogMessageW(dialog, &message) }.as_bool() {
            let _ = unsafe { TranslateMessage(&message) };
            unsafe { DispatchMessageW(&message) };
        }
    }

    Ok(())
}

fn get_installer(dialog: HWND) -> &'static mut Installer {
    unsafe { (GetWindowLongPtrW(dialog, GWLP_USERDATA) as *mut Installer).as_mut().unwrap() }
}

fn update_game_running_state(dialog: HWND) {
    let installer = get_installer(dialog);

    let is_running = match installer.game_version() {
        Some(GameVersion::DMM) => utils::is_specific_process_running("umamusume.exe"),
        Some(GameVersion::Steam) => utils::is_specific_process_running("UmamusumePrettyDerby_Jpn.exe"),
        Some(GameVersion::SteamGlobal) => utils::is_specific_process_running("UmamusumePrettyDerby.exe"),
        None => false,
    };

    let install_button = unsafe { GetDlgItem(dialog, IDC_INSTALL).unwrap() };
    let uninstall_button = unsafe { GetDlgItem(dialog, IDC_UNINSTALL).unwrap() };

    let is_installed = installer.is_current_target_installed();

    unsafe {
        let _ = EnableWindow(install_button, !is_running);
        let _ = EnableWindow(uninstall_button, !is_running && is_installed);
    }
}

fn refresh_all_target_labels(dialog: HWND) {
    let installer = get_installer(dialog);
    let target_combo = unsafe { GetDlgItem(dialog, IDC_TARGET).unwrap() };

    let current_idx = unsafe { SendMessageW(target_combo, CB_GETCURSEL, None, None).0 as usize };

    unsafe { SendMessageW(target_combo, CB_RESETCONTENT, None, None) };

    for (_i, target) in installer::Target::VALUES.into_iter().enumerate() {
        let label = installer.get_target_display_label(*target);
        unsafe {
            SendMessageW(
                target_combo, CB_ADDSTRING, None, LPARAM(HSTRING::from(label).as_ptr() as _)
            );
        }
    }

    unsafe { SendMessageW(target_combo, CB_SETCURSEL, WPARAM(current_idx), None) };
}

fn update_target(dialog: HWND, target_combo: HWND, index: usize) {
    let installer = get_installer(dialog);
    let target = installer::Target::VALUES[index];
    let mut installed = false;
    let label = if let Some(version_info) = installer.get_target_version_info(target) {
        installed = true;
        version_info.version.unwrap_or_else(|| "Unknown".to_owned())
    }
    else {
        "None".to_owned()
    };

    let installed_static = unsafe { GetDlgItem(dialog, IDC_INSTALLED).unwrap() };
    unsafe {
        _ = SetWindowTextW(installed_static, &HSTRING::from(t!("gui.installed", ver = label)));
        _ = EnableWindow(GetDlgItem(dialog, IDC_UNINSTALL).unwrap(), installed);
    }

    let label = installer.get_target_display_label(target);
    unsafe {
        SendMessageW(target_combo, CB_DELETESTRING, WPARAM(index), None);
        SendMessageW(target_combo, CB_INSERTSTRING, WPARAM(index), LPARAM(HSTRING::from(label).as_ptr() as _));
        SendMessageW(target_combo, CB_SETCURSEL, WPARAM(index), None);
    }

    installer.target = target;
    update_game_running_state(dialog);
}

unsafe extern "system" fn dlg_proc(dialog: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
    match message {
        WM_INITDIALOG => {
            // Set the installer ptr
            unsafe { SetWindowLongPtrW(dialog, GWLP_USERDATA, lparam.0) };
            let installer = unsafe { (lparam.0 as *mut Installer).as_mut().unwrap() };

            // Set icon
            let instance = unsafe { GetModuleHandleW(None).unwrap() };
            if let Ok(icon) = unsafe { LoadIconW(instance, IDI_HACHIMI) } {
                unsafe { SendMessageW(dialog, WM_SETICON, WPARAM(ICON_BIG as _), LPARAM(icon.0 as _)) };
                let _ = unsafe { DestroyIcon(icon) };
            }

            unsafe {
                SetTimer(dialog, ID_TIMER_GAMERUNNING, 1000, None);
            }

            let dmm_radio = unsafe { GetDlgItem(dialog, IDC_VERSION_DMM).ok() };
            let steam_radio = unsafe { GetDlgItem(dialog, IDC_VERSION_STEAM).ok() };
            let steam_global_radio = unsafe { GetDlgItem(dialog, IDC_VERSION_STEAM_GLOBAL).ok() };
            let version_group = unsafe { GetDlgItem(dialog, IDC_VERSION_GROUP).unwrap() };

            let has_dmm = installer.dmm_install_dir().is_some();
            let has_steam = installer.steam_install_dir().is_some();
            let has_steam_global = installer.steam_global_install_dir().is_some();

            let version_count = [has_dmm, has_steam, has_steam_global].iter().filter(|&&v| v).count();

            if version_count > 1 {
                let _ = unsafe { ShowWindow(version_group, SW_SHOW) };
                if has_dmm {
                    if let Some(btn) = dmm_radio {
                        let _ = unsafe { ShowWindow(btn, SW_SHOW) };
                    }
                }
                if has_steam {
                     if let Some(btn) = steam_radio {
                        let _ = unsafe { ShowWindow(btn, SW_SHOW) };
                    }
                }
                if has_steam_global {
                    if let Some(btn) = steam_global_radio {
                        let _ = unsafe { ShowWindow(btn, SW_SHOW) };
                    }
                }

                let initial_version = installer.game_version().unwrap_or(GameVersion::DMM);
                installer.set_game_version(initial_version);

                if let Some(path) = installer.install_dir() {
                    let install_path_edit = unsafe { GetDlgItem(dialog, IDC_INSTALL_PATH).unwrap() };
                    _ = unsafe { SetWindowTextW(install_path_edit, &HSTRING::from(path.to_str().unwrap())) };
                }

                let (dmm_check, steam_check, steam_global_check) = match initial_version {
                    GameVersion::DMM => (BST_CHECKED, BST_UNCHECKED, BST_UNCHECKED),
                    GameVersion::Steam => (BST_UNCHECKED, BST_CHECKED, BST_UNCHECKED),
                    GameVersion::SteamGlobal => (BST_UNCHECKED, BST_UNCHECKED, BST_CHECKED),
                };

                if let Some(btn) = dmm_radio {
                    unsafe { SendMessageW(btn, BM_SETCHECK, WPARAM(dmm_check.0 as _), None) };
                }
                if let Some(btn) = steam_radio {
                    unsafe { SendMessageW(btn, BM_SETCHECK, WPARAM(steam_check.0 as _), None) };
                }
                if let Some(btn) = steam_global_radio {
                    unsafe { SendMessageW(btn, BM_SETCHECK, WPARAM(steam_global_check.0 as _), None) };
                }
            } else {
                if let Some(path) = installer.install_dir() {
                    let install_path_edit = unsafe { GetDlgItem(dialog, IDC_INSTALL_PATH).unwrap() };
                    _ = unsafe { SetWindowTextW(install_path_edit, &HSTRING::from(path.to_str().unwrap())) };
                }
            }

            // Set packaged version
            let packaged_ver_static = unsafe { GetDlgItem(dialog, IDC_PACKAGED_VER).unwrap() };
            _ = unsafe {
                SetWindowTextW(
                    packaged_ver_static,
                    &HSTRING::from(t!("gui.packaged_ver", ver = env!("HACHIMI_VERSION")))
                )
            };

            localize_controls(dialog);

            // Init language combo
            let lang_combo = unsafe { GetDlgItem(dialog, IDC_LANGUAGE_COMBO).unwrap() };
            for (idx, (_, _, gui_label)) in SUPPORTED_LOCALES.iter().enumerate() {
                unsafe {
                    SendMessageW(
                        lang_combo, CB_ADDSTRING, None,
                        LPARAM(HSTRING::from(*gui_label).as_ptr() as _));
                }
                if SUPPORTED_LOCALES[idx].0 == *i18n::CURRENT_LOCALE.lock().unwrap() {
                    unsafe { SendMessageW(lang_combo, CB_SETCURSEL, WPARAM(idx), None) };
                }
            }

            let cur = i18n::CURRENT_LOCALE.lock().unwrap().clone();
            let idx = if cur == "zh-CN" { 1 } else { 0 };
            unsafe { SendMessageW(lang_combo, CB_SETCURSEL, WPARAM(idx), None) };

            // Init targets
            let target_combo = unsafe { GetDlgItem(dialog, IDC_TARGET).unwrap() };
            let mut default_target = if let Some(version) = installer.game_version() {
                match version {
                    installer::GameVersion::DMM => 0,
                    installer::GameVersion::Steam => 1,
                    installer::GameVersion::SteamGlobal => 1,
                }
            } else {
                0
            };
            let mut default_target_set = false;
            let mut multiple_installs = false;
            for (i, target) in installer::Target::VALUES.into_iter().enumerate() {
                let label = if let Some(version_info) = installer.get_target_version_info(*target) {
                    if version_info.is_hachimi() {
                        if default_target_set {
                            // Already set; multiple installations detected!
                            multiple_installs = true;
                        }
                        default_target = i;
                        default_target_set = true;
                    }
                    version_info.get_display_label(*target)
                }
                else {
                    target.dll_name().to_owned()
                };
                unsafe {
                    SendMessageW(
                        target_combo, CB_ADDSTRING, None, LPARAM(HSTRING::from(label).as_ptr() as _)
                    );
                }
            }
            // Defaults to already installed Hachimi dll, if any
            update_target(dialog, target_combo, default_target);

            // Show notice if install dir is not detected
            if installer.install_dir().is_none() {
                unsafe {
                    MessageBoxW(
                        dialog,
                        &HSTRING::from(t!("gui.warning_no_dir")),
                        &HSTRING::from(t!("gui.warning")),
                        MB_ICONWARNING | MB_OK
                    );
                }
            }

            // Show notice for multiple installs
            if multiple_installs {
                unsafe {
                    MessageBoxW(
                        dialog,
                        &HSTRING::from(t!("gui.warning_multi-installation")),
                        &HSTRING::from(t!("gui.warning")),
                        MB_ICONWARNING | MB_OK
                    );
                }
            }

            1
        },

        WM_COMMAND => {
            let control_id = wparam.0 as i16 as i32;
            let notif_code = wparam.0 as u32 >> 16;
            let ncode = ((wparam.0 >> 16) & 0xFFFF) as u32;
            let control = HWND(lparam.0 as _);

            match control_id {
                IDC_VERSION_DMM | IDC_VERSION_STEAM | IDC_VERSION_STEAM_GLOBAL => {
                    let installer = get_installer(dialog);
                    let version = if control_id == IDC_VERSION_DMM {
                        GameVersion::DMM
                    } else if control_id == IDC_VERSION_STEAM {
                        GameVersion::Steam
                    } else {
                        GameVersion::SteamGlobal
                    };

                    if let Some(path) = installer.set_game_version(version) {
                        let install_path_edit = unsafe { GetDlgItem(dialog, IDC_INSTALL_PATH).unwrap() };
                        _ = unsafe { SetWindowTextW(install_path_edit, &HSTRING::from(path.to_str().unwrap())) };
                    }

                    if let Ok(btn) = unsafe { GetDlgItem(dialog, IDC_VERSION_DMM) } {
                        unsafe { SendMessageW(btn, BM_SETCHECK, WPARAM(BST_UNCHECKED.0 as _), None) };
                    }
                    if let Ok(btn) = unsafe { GetDlgItem(dialog, IDC_VERSION_STEAM) } {
                        unsafe { SendMessageW(btn, BM_SETCHECK, WPARAM(BST_UNCHECKED.0 as _), None) };
                    }
                    if let Ok(btn) = unsafe { GetDlgItem(dialog, IDC_VERSION_STEAM_GLOBAL) } {
                        unsafe { SendMessageW(btn, BM_SETCHECK, WPARAM(BST_UNCHECKED.0 as _), None) };
                    }

                    unsafe { SendMessageW(control, BM_SETCHECK, WPARAM(BST_CHECKED.0 as _), None) };

                    let _ = unsafe { EnableWindow(GetDlgItem(dialog, IDC_INSTALL).unwrap(), true) };
                    let _ = unsafe { EnableWindow(GetDlgItem(dialog, IDC_UNINSTALL).unwrap(), true) };
                    let _ = unsafe { EnableWindow(GetDlgItem(dialog, IDC_INSTALL_PATH_BROWSE).unwrap(), true) };

                    refresh_all_target_labels(dialog);

                    let new_default_target = if let Some(v) = installer.game_version() {
                        match v {
                            GameVersion::DMM => 0,
                            GameVersion::Steam | GameVersion::SteamGlobal => 1,
                        }
                    } else {
                        0
                    };

                    update_target(dialog, unsafe { GetDlgItem(dialog, IDC_TARGET).unwrap() }, new_default_target);
                }

                IDC_LANGUAGE_COMBO if ncode == CBN_SELCHANGE => {
                    let combo = unsafe { GetDlgItem(dialog, IDC_LANGUAGE_COMBO).unwrap() };
                    let idx   = unsafe { SendMessageW(combo, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as usize };
                    i18n::set_locale(SUPPORTED_LOCALES[idx].0);
                    localize_controls(dialog);

                    refresh_all_target_labels(dialog);
                    let current_target_idx = get_installer(dialog).target as usize;
                    update_target(dialog, unsafe { GetDlgItem(dialog, IDC_TARGET).unwrap() }, current_target_idx);
                }
                IDC_INSTALL_PATH_BROWSE => {
                    let installer = get_installer(dialog);
                    let Some(path) = utils::open_select_folder_dialog(
                        dialog,
                        installer.install_dir().filter(|p| p.is_dir())
                    ) else {
                        return 1;
                    };

                    match installer.set_install_dir(path.clone()) {
                        Ok(_) => {
                            let install_path_edit = unsafe { GetDlgItem(dialog, IDC_INSTALL_PATH).unwrap() };
                            _ = unsafe { SetWindowTextW(install_path_edit, &HSTRING::from(path.to_str().unwrap())) };

                            // Update radio buttons to reflect the detected game version
                            if let Some(version) = installer.game_version() {
                                let (dmm_check, steam_check, steam_global_check) = match version {
                                    GameVersion::DMM => (BST_CHECKED, BST_UNCHECKED, BST_UNCHECKED),
                                    GameVersion::Steam => (BST_UNCHECKED, BST_CHECKED, BST_UNCHECKED),
                                    GameVersion::SteamGlobal => (BST_UNCHECKED, BST_UNCHECKED, BST_CHECKED),
                                };

                                if let Ok(btn) = unsafe { GetDlgItem(dialog, IDC_VERSION_DMM) } {
                                    unsafe { SendMessageW(btn, BM_SETCHECK, WPARAM(dmm_check.0 as _), None) };
                                }
                                if let Ok(btn) = unsafe { GetDlgItem(dialog, IDC_VERSION_STEAM) } {
                                    unsafe { SendMessageW(btn, BM_SETCHECK, WPARAM(steam_check.0 as _), None) };
                                }
                                if let Ok(btn) = unsafe { GetDlgItem(dialog, IDC_VERSION_STEAM_GLOBAL) } {
                                    unsafe { SendMessageW(btn, BM_SETCHECK, WPARAM(steam_global_check.0 as _), None) };
                                }
                            }
                        }
                        Err(e) => {
                            unsafe { MessageBoxW(dialog, &HSTRING::from(e.to_string()), &HSTRING::from(t!("gui.error")), MB_ICONERROR | MB_OK) };
                        }
                    }

                    refresh_all_target_labels(dialog);

                    let new_default_target = if let Some(v) = installer.game_version() {
                        match v {
                            GameVersion::DMM => 0,
                            GameVersion::Steam | GameVersion::SteamGlobal => 1,
                        }
                    } else {
                        0
                    };

                    update_target(dialog, unsafe { GetDlgItem(dialog, IDC_TARGET).unwrap() }, new_default_target);
                }

                IDC_TARGET => {
                    if notif_code == CBN_SELCHANGE {
                        let res = unsafe { SendMessageW(control, CB_GETCURSEL, None, None) };
                        update_target(dialog, control, res.0 as _);
                    }
                }

                IDC_INSTALL => {
                    let installer = get_installer(dialog);
                    if let Some(target) = installer.get_hachimi_installed_target() {
                        if target != installer.target {
                            unsafe {
                                MessageBoxW(
                                    dialog,
                                    &HSTRING::from(t!("gui.already_installed", dll = target.dll_name())),
                                    &HSTRING::from(t!("gui.error")),
                                    MB_ICONERROR | MB_OK
                                );
                            }
                            return 0;
                        }
                    }
                    if installer.is_current_target_installed() {
                        let res = unsafe {
                            MessageBoxW(
                                dialog,
                                &HSTRING::from(t!("gui.replace_confirm", dll = installer.target.dll_name())),
                                &HSTRING::from(t!("gui.install")),
                                MB_ICONINFORMATION | MB_OKCANCEL
                            )
                        };
                        if res != IDOK {
                            return 0;
                        }
                    }

                    let (is_running, exe_name) = match installer.game_version() {
                        Some(GameVersion::DMM) => (utils::is_specific_process_running("umamusume.exe"), "umamusume.exe"),
                        Some(GameVersion::Steam) => (utils::is_specific_process_running("UmamusumePrettyDerby_Jpn.exe"), "UmamusumePrettyDerby_Jpn.exe"),
                        Some(GameVersion::SteamGlobal) => (utils::is_specific_process_running("UmamusumePrettyDerby.exe"), "UmamusumePrettyDerby.exe"),
                        None => (false, "the game")
                    };

                    if is_running {
                        unsafe {
                            MessageBoxW(
                                dialog,
                                &HSTRING::from(t!("gui.error_app_running_body", app_name = exe_name)),
                                &HSTRING::from(t!("gui.title")),
                                MB_ICONERROR | MB_OK
                            );
                        }
                        return 0;
                    }

                    match installer.pre_install()
                        .and_then(|_| installer.install())
                        .and_then(|_| installer.post_install())
                    {
                        Ok(_) => {
                            unsafe { MessageBoxW(dialog, &HSTRING::from(t!("gui.msg_install_ok")), &HSTRING::from(t!("gui.title")), MB_ICONINFORMATION | MB_OK) };
                        },
                        Err(e) => {
                            unsafe { MessageBoxW(dialog, &HSTRING::from(t!("gui.msg_install_fail", err = e.to_string())), &HSTRING::from(t!("gui.title")), MB_ICONERROR | MB_OK) };
                        }
                    }
                    refresh_all_target_labels(dialog);
                    update_target(dialog, unsafe { GetDlgItem(dialog, IDC_TARGET).unwrap() }, installer.target as _);
                }

                IDC_UNINSTALL => {
                    let installer = get_installer(dialog);

                    let (is_running, exe_name) = match installer.game_version() {
                        Some(GameVersion::DMM) => (utils::is_specific_process_running("umamusume.exe"), "umamusume.exe"),
                        Some(GameVersion::Steam) => (utils::is_specific_process_running("UmamusumePrettyDerby_Jpn.exe"), "UmamusumePrettyDerby_Jpn.exe"),
                        Some(GameVersion::SteamGlobal) => (utils::is_specific_process_running("UmamusumePrettyDerby.exe"), "UmamusumePrettyDerby.exe"),
                        None => (false, "the game")
                    };

                    if is_running {
                        unsafe {
                            MessageBoxW(
                                dialog,
                                &HSTRING::from(t!("gui.error_app_running_body", app_name = exe_name)),
                                &HSTRING::from(t!("gui.title")),
                                MB_ICONERROR | MB_OK
                            );
                        }
                        return 0;
                    }

                    let res = unsafe {
                        MessageBoxW(
                            dialog,
                            &HSTRING::from(t!("gui.delete_confirm", dll = installer.target.dll_name())),
                            &HSTRING::from(t!("gui.uninstall")),
                            MB_ICONINFORMATION | MB_OKCANCEL
                        )
                    };
                    if res == IDOK {
                        let version_info_opt = installer.get_target_version_info(installer.target);
                        if let Err(e) = installer.uninstall() {
                            unsafe { MessageBoxW(dialog, &HSTRING::from(e.to_string()), &HSTRING::from(t!("gui.error")), MB_ICONERROR | MB_OK) };
                            return 0;
                        }

                        refresh_all_target_labels(dialog);
                        update_target(dialog, unsafe { GetDlgItem(dialog, IDC_TARGET).unwrap() }, installer.target as _);

                        if let Some(version_info) = version_info_opt {
                            if !version_info.is_hachimi() {
                                return 0;

                            }

                            // Check if the hachimi data dir exists and prompt user to delete it
                            let hachimi_dir = installer.install_dir().as_ref().unwrap().join("hachimi");
                            let Ok(metadata) = std::fs::metadata(&hachimi_dir) else {
                                return 0;
                            };

                            if metadata.is_dir() {
                                let res = unsafe {
                                    MessageBoxW(
                                        dialog,
                                        &HSTRING::from(t!("gui.delete_data_dir")),
                                        &HSTRING::from(t!("gui.uninstall")),
                                        MB_ICONINFORMATION | MB_YESNO
                                    )
                                };

                                if res == IDYES {
                                    if let Err(e) = std::fs::remove_dir_all(&hachimi_dir) {
                                        unsafe { MessageBoxW(dialog, &HSTRING::from(e.to_string()), &HSTRING::from(t!("gui.error")), MB_ICONERROR | MB_OK) };
                                        return 0;
                                    }
                                }
                            }
                        }
                    }
                }

                _ => return 0
            }

            1
        }

        WM_CLOSE => {
            let _ = unsafe { KillTimer(dialog, ID_TIMER_GAMERUNNING) };
            unsafe { PostQuitMessage(0) };
            0
        }

        _ => 0
    }
}