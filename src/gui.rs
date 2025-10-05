use crate::{installer::{self, GameVersion, Installer}, resource::*, updater::UpdateStatus, utils};
use windows::{core::{w, HSTRING}, Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::{BST_CHECKED, BST_UNCHECKED}, Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::{
        BM_SETCHECK, CreateDialogParamW, DestroyIcon, DispatchMessageW, GetDlgItem, GetMessageW,
        GetWindowLongPtrW, LoadIconW, MessageBoxW, PostQuitMessage, SendMessageW,
        SetWindowLongPtrW,SetWindowTextW, ShowWindow, TranslateMessage,
        CBN_SELCHANGE, CB_ADDSTRING, CB_DELETESTRING, CB_GETCURSEL, CB_INSERTSTRING, CB_SETCURSEL,
        GWLP_USERDATA, ICON_BIG, IDOK, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING,
        MB_OK, MB_OKCANCEL, MB_YESNO, MSG, SW_SHOW, WM_CLOSE, WM_COMMAND, WM_INITDIALOG, WM_SETICON
    }}
}};

pub fn run(update_status: UpdateStatus) -> Result<(), windows::core::Error> {
    match update_status {
        UpdateStatus::Updated(date) => {
            let message = format!(
                "Successfully updated to the latest nightly build (from {}).\n\nPlease restart the application to use the new version.",
                date
            );
            unsafe {
                MessageBoxW(None, &HSTRING::from(message), w!("Update Successful"), MB_ICONINFORMATION | MB_OK);
            }
            std::process::exit(0);
        }
        UpdateStatus::NotNeeded | UpdateStatus::Disabled => {}
        UpdateStatus::Failed(msg) => {
            let message = format!("Failed to check for updates:\n\n{}", msg);
            unsafe {
                MessageBoxW(None, &HSTRING::from(message), w!("Update Error"), MB_ICONERROR | MB_OK);
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
    unsafe {
        while GetMessageW(&mut message, None, 0, 0).into() {
            _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    Ok(())
}

fn get_installer(dialog: HWND) -> &'static mut Installer {
    unsafe { (GetWindowLongPtrW(dialog, GWLP_USERDATA) as *mut Installer).as_mut().unwrap() }
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
        _ = SetWindowTextW(installed_static, &HSTRING::from(format!("Installed: {}", label)));
        _ = EnableWindow(GetDlgItem(dialog, IDC_UNINSTALL).unwrap(), installed);
    }
        

    let label = installer.get_target_display_label(target);
    unsafe {
        SendMessageW(target_combo, CB_DELETESTRING, WPARAM(index), None);
        SendMessageW(target_combo, CB_INSERTSTRING, WPARAM(index), LPARAM(HSTRING::from(label).as_ptr() as _));
        SendMessageW(target_combo, CB_SETCURSEL, WPARAM(index), None);
    }

    installer.target = target;
}

unsafe extern "system" fn dlg_proc(dialog: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
    match message {
        WM_INITDIALOG => {
            // Set the installer ptr
            unsafe { SetWindowLongPtrW(dialog, GWLP_USERDATA, lparam.0) };
            let _installer = unsafe { (lparam.0 as *mut Installer).as_ref().unwrap() };

            // Set icon
            let instance = unsafe { GetModuleHandleW(None).unwrap() };
            if let Ok(icon) = unsafe { LoadIconW(instance, IDI_HACHIMI) } {
                unsafe { SendMessageW(dialog, WM_SETICON, WPARAM(ICON_BIG.0 as _), LPARAM(icon.0 as _)) };
                let _ = unsafe { DestroyIcon(icon) };
            }

            let installer = get_installer(dialog);
            let dmm_radio = unsafe { GetDlgItem(dialog, IDC_VERSION_DMM).unwrap() };
            let steam_radio = unsafe { GetDlgItem(dialog, IDC_VERSION_STEAM).unwrap() };
            let version_group = unsafe { GetDlgItem(dialog, IDC_VERSION_GROUP).unwrap() };

            let has_dmm = installer.dmm_install_dir().is_some();
            let has_steam = installer.steam_install_dir().is_some();

            if has_dmm && has_steam {
                let _ = unsafe { ShowWindow(version_group, SW_SHOW) };
                let _ = unsafe { ShowWindow(dmm_radio, SW_SHOW) };
                let _ = unsafe { ShowWindow(steam_radio, SW_SHOW) };

                let _ = unsafe { EnableWindow(GetDlgItem(dialog, IDC_INSTALL).unwrap(), false) };
                let _ = unsafe { EnableWindow(GetDlgItem(dialog, IDC_UNINSTALL).unwrap(), false) };
                let _ = unsafe { EnableWindow(GetDlgItem(dialog, IDC_INSTALL_PATH_BROWSE).unwrap(), false) };

            } else {
                if let Some(path) = installer.install_dir() {
                    let install_path_edit = unsafe { GetDlgItem(dialog, IDC_INSTALL_PATH).unwrap() };
                    _ = unsafe { SetWindowTextW(install_path_edit, &HSTRING::from(path.to_str().unwrap())) };
                }
            }

            // Set install path
            if let Some(path) = installer.install_dir() {
                let install_path_edit = unsafe { GetDlgItem(dialog, IDC_INSTALL_PATH).unwrap() };
                _ = unsafe { SetWindowTextW(install_path_edit, &HSTRING::from(path.to_str().unwrap())) };
            }

            // Set packaged version
            let packaged_ver_static = unsafe { GetDlgItem(dialog, IDC_PACKAGED_VER).unwrap() };
            _ = unsafe {
                SetWindowTextW(
                    packaged_ver_static,
                    &HSTRING::from(format!("Packaged version: {}", env!("HACHIMI_VERSION")))
                )
            };

            // Init targets
            let target_combo = unsafe { GetDlgItem(dialog, IDC_TARGET).unwrap() };
            let mut default_target = 0;
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
                        w!("Failed to detect the game's install location. Please select it manually."),
                        w!("Warning"),
                        MB_ICONWARNING | MB_OK
                    );
                }
            }

            // Show notice for multiple installs
            if multiple_installs {
                unsafe {
                    MessageBoxW(
                        dialog,
                        w!("Multiple installations of Hachimi detected! \
                            Please uninstall one of them, otherwise the game will not work correctly."),
                        w!("Warning"),
                        MB_ICONWARNING | MB_OK
                    );
                }
            }

            1
        },

        WM_COMMAND => {
            let control_id = wparam.0 as i16 as i32;
            let notif_code = wparam.0 as u32 >> 16;
            let control = HWND(lparam.0 as _);

            match control_id {
                IDC_VERSION_DMM | IDC_VERSION_STEAM => {
                    let installer = get_installer(dialog);
                    let version = if control_id == IDC_VERSION_DMM {
                        GameVersion::DMM
                    } else {
                        GameVersion::Steam
                    };
                    installer.set_game_version(version);

                    let dmm_radio = unsafe { GetDlgItem(dialog, IDC_VERSION_DMM).unwrap() };
                    let steam_radio = unsafe { GetDlgItem(dialog, IDC_VERSION_STEAM).unwrap() };
                    unsafe { SendMessageW(dmm_radio, BM_SETCHECK, WPARAM(BST_UNCHECKED.0 as _), None) };
                    unsafe { SendMessageW(steam_radio, BM_SETCHECK, WPARAM(BST_UNCHECKED.0 as _), None) };
                    unsafe { SendMessageW(control, BM_SETCHECK, WPARAM(BST_CHECKED.0 as _), None) };

                    if let Some(path) = installer.install_dir() {
                        let install_path_edit = unsafe { GetDlgItem(dialog, IDC_INSTALL_PATH).unwrap() };
                        _ = unsafe { SetWindowTextW(install_path_edit, &HSTRING::from(path.to_str().unwrap())) };
                    }

                    let _ = unsafe { EnableWindow(GetDlgItem(dialog, IDC_INSTALL).unwrap(), true) };
                    let _ = unsafe { EnableWindow(GetDlgItem(dialog, IDC_UNINSTALL).unwrap(), true) };
                    let _ = unsafe { EnableWindow(GetDlgItem(dialog, IDC_INSTALL_PATH_BROWSE).unwrap(), true) };

                    update_target(dialog, unsafe { GetDlgItem(dialog, IDC_TARGET).unwrap() }, installer.target as _);
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
                            }
                            Err(e) => {
                                unsafe { MessageBoxW(dialog, &HSTRING::from(e.to_string()), w!("Error"), MB_ICONERROR | MB_OK) };
                            }
                        }

                    update_target(dialog, unsafe { GetDlgItem(dialog, IDC_TARGET).unwrap() }, installer.target as _);
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
                                    &HSTRING::from(format!("Hachimi is already installed as {}", target.dll_name())),
                                    w!("Error"),
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
                                &HSTRING::from(format!("Replace {}?", installer.target.dll_name())),
                                w!("Install"),
                                MB_ICONINFORMATION | MB_OKCANCEL
                            )
                        };
                        if res != IDOK {
                            return 0;
                        }
                    }
                    match installer.pre_install()
                        .and_then(|_| installer.install())
                        .and_then(|_| installer.post_install())
                    {
                        Ok(_) => {
                            unsafe { MessageBoxW(dialog, w!("Install completed."), w!("Success"), MB_ICONINFORMATION | MB_OK) };
                        },
                        Err(e) => {
                            unsafe { MessageBoxW(dialog, &HSTRING::from(e.to_string()), w!("Error"), MB_ICONERROR | MB_OK) };
                        }
                    }
                    update_target(dialog, unsafe { GetDlgItem(dialog, IDC_TARGET).unwrap() }, installer.target as _);
                }

                IDC_UNINSTALL => {
                    let installer = get_installer(dialog);
                    let res = unsafe {
                        MessageBoxW(
                            dialog,
                            &HSTRING::from(format!("Delete {}?", installer.target.dll_name())),
                            w!("Uninstall"),
                            MB_ICONINFORMATION | MB_OKCANCEL
                        )
                    };
                    if res == IDOK {
                        let version_info_opt = installer.get_target_version_info(installer.target);
                        if let Err(e) = installer.uninstall() {
                            unsafe { MessageBoxW(dialog, &HSTRING::from(e.to_string()), w!("Error"), MB_ICONERROR | MB_OK) };
                            return 0;
                        }
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
                                        w!("Do you also want to delete Hachimi's data directory?"),
                                        w!("Uninstall"),
                                        MB_ICONINFORMATION | MB_YESNO
                                    )
                                };

                                if res == IDYES {
                                    if let Err(e) = std::fs::remove_dir_all(&hachimi_dir) {
                                        unsafe { MessageBoxW(dialog, &HSTRING::from(e.to_string()), w!("Error"), MB_ICONERROR | MB_OK) };
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
            unsafe { PostQuitMessage(0) };
            0
        }

        _ => 0
    }
}