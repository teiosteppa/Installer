use crate::{installer::{self, Installer}, resource::*, utils};
use crate::i18n::{self, SUPPORTED_LOCALES, t};
use windows::{core::{HSTRING}, Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::{
        CreateDialogParamW, DestroyIcon, DispatchMessageW, GetDlgItem, GetMessageW,
        GetWindowLongPtrW, LoadIconW, MessageBoxW, PostQuitMessage, SendMessageW,
        SetWindowLongPtrW,SetWindowTextW, ShowWindow, TranslateMessage,
        CBN_SELCHANGE, CB_ADDSTRING, CB_DELETESTRING, CB_GETCURSEL, CB_INSERTSTRING, CB_SETCURSEL,
        GWLP_USERDATA, ICON_BIG, IDOK, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING,
        MB_OK, MB_OKCANCEL, MB_YESNO, MSG, SW_SHOW, WM_CLOSE, WM_COMMAND, WM_INITDIALOG, WM_SETICON
    }},
}};

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

    }
}

pub fn run() -> Result<(), windows::core::Error> {
    let mut installer = Box::new(Installer::default());

    let instance = unsafe { GetModuleHandleW(None)? };
    let dialog = unsafe {
        CreateDialogParamW(instance, IDD_MAIN, None, Some(dlg_proc), LPARAM(installer.as_mut() as *mut _ as _))
    }?;
    utils::center_window(dialog)?;
    unsafe { _ = ShowWindow(dialog, SW_SHOW) };
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
}

unsafe extern "system" fn dlg_proc(dialog: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> isize {
    match message {
        WM_INITDIALOG => {
            // Set the installer ptr
            SetWindowLongPtrW(dialog, GWLP_USERDATA, lparam.0);
            let installer = unsafe { (lparam.0 as *mut Installer).as_ref().unwrap() };

            // Set icon
            let instance = unsafe { GetModuleHandleW(None).unwrap() };
            if let Ok(icon) = LoadIconW(instance, IDI_HACHIMI) {
                SendMessageW(dialog, WM_SETICON, WPARAM(ICON_BIG as _), LPARAM(icon.0 as _));
                _ = DestroyIcon(icon);
            }

            // Set install path
            if let Some(path) = &installer.install_dir {
                let install_path_edit = GetDlgItem(dialog, IDC_INSTALL_PATH).unwrap();
                _ = SetWindowTextW(install_path_edit, &HSTRING::from(path.to_str().unwrap()));
            }

            // Set packaged version
            let packaged_ver_static = GetDlgItem(dialog, IDC_PACKAGED_VER).unwrap();
            _ = SetWindowTextW(
                packaged_ver_static,
                &HSTRING::from(t!("gui.packaged_ver", ver = env!("HACHIMI_VERSION")))
            );

            localize_controls(dialog);

            // Init language combo
            let lang_combo = GetDlgItem(dialog, IDC_LANGUAGE_COMBO).unwrap();
            for (idx, (_, _, gui_label)) in SUPPORTED_LOCALES.iter().enumerate() {
                SendMessageW(
                    lang_combo, CB_ADDSTRING, None,
                    LPARAM(HSTRING::from(*gui_label).as_ptr() as _));
                if SUPPORTED_LOCALES[idx].0 == *i18n::CURRENT_LOCALE.lock().unwrap() {
                    SendMessageW(lang_combo, CB_SETCURSEL, WPARAM(idx), None);
                }
            }

            let cur = i18n::CURRENT_LOCALE.lock().unwrap().clone();
            let idx = if cur == "zh-CN" { 1 } else { 0 };
            SendMessageW(lang_combo, CB_SETCURSEL, WPARAM(idx), None);

            // Init targets
            let target_combo = GetDlgItem(dialog, IDC_TARGET).unwrap();
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
                SendMessageW(
                    target_combo, CB_ADDSTRING, None, LPARAM(HSTRING::from(label).as_ptr() as _)
                );
            }
            // Defaults to already installed Hachimi dll, if any
            update_target(dialog, target_combo, default_target);

            // Show notice if install dir is not detected
            if installer.install_dir.is_none() {
                MessageBoxW(
                    dialog,
                    &HSTRING::from(t!("gui.warning_no_dir")),
                    &HSTRING::from(t!("gui.warning")),
                    MB_ICONWARNING | MB_OK
                );
            }

            // Show notice for multiple installs
            if multiple_installs {
                MessageBoxW(
                    dialog,
                    &HSTRING::from(t!("gui.warning_multi-installation")),
                    &HSTRING::from(t!("gui.warning")),
                    MB_ICONWARNING | MB_OK
                );
            }

            1
        },

        WM_COMMAND => {
            let control_id = wparam.0 as i16 as i32;
            let notif_code = wparam.0 as u32 >> 16;
            let ncode = ((wparam.0 >> 16) & 0xFFFF) as u32;
            let control = HWND(lparam.0 as _);

            match control_id {
                IDC_LANGUAGE_COMBO if ncode == CBN_SELCHANGE => {
                    let combo = GetDlgItem(dialog, IDC_LANGUAGE_COMBO).unwrap();
                    let idx   = SendMessageW(combo, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as usize;
                    i18n::set_locale(SUPPORTED_LOCALES[idx].0);
                    localize_controls(dialog);
                    update_target(dialog, GetDlgItem(dialog, IDC_TARGET).unwrap(),
                                  get_installer(dialog).target as _);
                }
                IDC_INSTALL_PATH_BROWSE => {
                    let installer = get_installer(dialog);
                    let Some(path) = utils::open_select_folder_dialog(
                        dialog,
                        installer.install_dir.as_ref().filter(|p| p.is_dir())
                    ) else {
                        return 1;
                    };

                    let install_path_edit = GetDlgItem(dialog, IDC_INSTALL_PATH).unwrap();
                    _ = SetWindowTextW(install_path_edit, &HSTRING::from(path.to_str().unwrap()));

                    installer.install_dir = Some(path);
                    update_target(dialog, GetDlgItem(dialog, IDC_TARGET).unwrap(), installer.target as _);
                }

                IDC_TARGET => {
                    if notif_code == CBN_SELCHANGE {
                        let res = SendMessageW(control, CB_GETCURSEL, None, None);
                        update_target(dialog, control, res.0 as _);
                    }
                }

                IDC_INSTALL => {
                    let installer = get_installer(dialog);
                    if let Some(target) = installer.get_hachimi_installed_target() {
                        if target != installer.target {
                            MessageBoxW(
                                dialog,
                                &HSTRING::from(t!("gui.already_installed", dll = target.dll_name())),
                                &HSTRING::from(t!("gui.error")),

                                MB_ICONERROR | MB_OK
                            );
                            return 0;
                        }
                    }
                    if installer.is_current_target_installed() {
                        let res = MessageBoxW(
                            dialog,
                            &HSTRING::from(t!("gui.replace_confirm", dll = installer.target.dll_name())),
                            &HSTRING::from(t!("gui.install")),
                            MB_ICONINFORMATION | MB_OKCANCEL
                        );
                        if res != IDOK {
                            return 0;
                        }
                    }
                    match installer.pre_install()
                        .and_then(|_| installer.install())
                        .and_then(|_| installer.post_install())
                    {
                        Ok(_) => {
                            MessageBoxW(dialog, &HSTRING::from(t!("gui.msg_install_ok")),  
                                              &HSTRING::from(t!("gui.title")), MB_ICONINFORMATION | MB_OK);
                        },
                        Err(e) => {
                            MessageBoxW(dialog, &HSTRING::from(t!("gui.msg_install_fail", err = e.to_string())),
                                              &HSTRING::from(t!("gui.title")), MB_ICONERROR | MB_OK);
                        }
                    }
                    update_target(dialog, GetDlgItem(dialog, IDC_TARGET).unwrap(), installer.target as _);
                }

                IDC_UNINSTALL => {
                    let installer = get_installer(dialog);
                    let res = MessageBoxW(
                        dialog,
                        &HSTRING::from(t!("gui.delete_confirm", dll = installer.target.dll_name())),
                        &HSTRING::from(t!("gui.uninstall")),
                        MB_ICONINFORMATION | MB_OKCANCEL
                    );
                    if res == IDOK {
                        let version_info_opt = installer.get_target_version_info(installer.target);
                        if let Err(e) = installer.uninstall() {
                            MessageBoxW(dialog, &HSTRING::from(e.to_string()), &HSTRING::from(t!("gui.error")), MB_ICONERROR | MB_OK);
                            return 0;
                        }
                        update_target(dialog, GetDlgItem(dialog, IDC_TARGET).unwrap(), installer.target as _);

                        if let Some(version_info) = version_info_opt {
                            if !version_info.is_hachimi() {
                                return 0;
                            }

                            // Check if the hachimi data dir exists and prompt user to delete it
                            let hachimi_dir = installer.install_dir.as_ref().unwrap().join("hachimi");
                            let Ok(metadata) = std::fs::metadata(&hachimi_dir) else {
                                return 0;
                            };

                            if metadata.is_dir() {
                                let res = MessageBoxW(
                                    dialog,
                                    &HSTRING::from(t!("gui.delete_data_dir")),
                                    &HSTRING::from(t!("gui.uninstall")),

                                    MB_ICONINFORMATION | MB_YESNO
                                );

                                if res == IDYES {
                                    if let Err(e) = std::fs::remove_dir_all(&hachimi_dir) {
                                        MessageBoxW(dialog, &HSTRING::from(e.to_string()), &HSTRING::from(t!("gui.error")), MB_ICONERROR | MB_OK);
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
            PostQuitMessage(0);
            0
        }

        _ => 0
    }
}