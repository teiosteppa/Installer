use crate::{installer::{self, Installer}, resource::*, utils};
#[cfg(feature = "net_install")]
use std::thread;
#[cfg(feature = "net_install")]
use tinyjson::JsonValue;
use windows::{core::{w, HSTRING}, Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{Input::KeyboardAndMouse::EnableWindow, WindowsAndMessaging::{
        CreateDialogParamW, DestroyIcon, DispatchMessageW, GetDlgItem, GetMessageW,
        GetWindowLongPtrW, LoadIconW, MessageBoxW, PostQuitMessage, SendMessageW,
        SetWindowLongPtrW,SetWindowTextW, ShowWindow, TranslateMessage,
        CBN_SELCHANGE, CB_ADDSTRING, CB_DELETESTRING, CB_GETCURSEL, CB_INSERTSTRING, CB_SETCURSEL,
        GWLP_USERDATA, ICON_BIG, IDOK, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING,
        MB_OK, MB_OKCANCEL, MB_YESNO, MSG, SW_SHOW, WM_CLOSE, WM_COMMAND, WM_INITDIALOG, WM_SETICON
    }}
}};

pub fn run() -> Result<(), windows::core::Error> {
    let mut installer = Box::new(Installer::default());

    #[cfg(feature = "net_install")]
    {
        // fetch version synchronously before creating the GUI
        let client = reqwest::blocking::Client::builder()
            .user_agent("hachimi-installer")
            .build()
            .unwrap();

        let get_version = || -> Result<String, Box<dyn std::error::Error>> {
            let resp_text = client.get("https://api.github.com/repos/kairusds/Hachimi-Edge/releases/latest").send()?.text()?;
            let json: JsonValue = resp_text.parse()?;

            // stole from lead
            let JsonValue::Object(root_obj) = json else {
                return Err("GitHub API response was not a JSON object".into());
            };

            let JsonValue::String(tag_name) = &root_obj["tag_name"] else {
                return Err("tag_name field not found or not a string in API response".into());
            };

            Ok(tag_name.clone())
        };

        match get_version() {
            Ok(version) => {
                *installer.hachimi_version.lock().unwrap() = Some(version);
            }
            Err(_) => {
                unsafe { MessageBoxW(None, w!("Unable to get Hachimi version from GitHub. Are you online?"), w!("Error"), MB_ICONERROR | MB_OK) };
                return Ok(());
            }
        }
    }

    #[cfg(feature = "net_install")]
    {
        let dll_handle = installer.hachimi_dll.clone();
        thread::spawn(move || {
            let result = reqwest::blocking::get("https://github.com/kairusds/Hachimi-Edge/releases/latest/download/hachimi.dll")
                .and_then(|resp| resp.bytes());
            *dll_handle.lock().unwrap() = Some(result);
        });
    }
    let instance = unsafe { GetModuleHandleW(None)? };
    let dialog = unsafe {
        CreateDialogParamW(instance, IDD_MAIN, None, Some(dlg_proc), LPARAM(installer.as_mut() as *mut _ as _))
    }?;
    utils::center_window(dialog)?;
    unsafe { _ = ShowWindow(dialog, SW_SHOW) };
    *installer.hwnd.lock().unwrap() = Some(dialog);

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

    installer.install_dir = Installer::detect_install_dir(target);
    if let Some(path) = &installer.install_dir {
        let install_path_edit = unsafe { GetDlgItem(dialog, IDC_INSTALL_PATH).unwrap() };
        unsafe { let _ = SetWindowTextW(install_path_edit, &HSTRING::from(path.to_str().unwrap())); };
    }

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
            #[cfg(not(feature = "net_install"))]
            {
                let packaged_ver_static = GetDlgItem(dialog, IDC_PACKAGED_VER).unwrap();
                _ = SetWindowTextW(
                    packaged_ver_static,
                    &HSTRING::from(format!("Available version: {}", env!("HACHIMI_VERSION")))
                );
            }
            #[cfg(feature = "net_install")]
            {
                let version_str = installer.hachimi_version.lock().unwrap()
                    .as_deref()
                    .unwrap_or("Error")
                    .to_string();
                let packaged_ver_static = GetDlgItem(dialog, IDC_PACKAGED_VER).unwrap();
                _ = SetWindowTextW(packaged_ver_static, &HSTRING::from(format!("Available version: {}", version_str)));
            }

            // Init targets
            let target_combo = GetDlgItem(dialog, IDC_TARGET).unwrap();
            let dmm_installed = installer::detect_dmm_install_dir().is_some();
            let steam_installed = installer::detect_steam_install_dir().is_some();
            let installed_count = [dmm_installed, steam_installed].iter().filter(|&&x| x).count();

            for target in installer::Target::VALUES {
                let label = installer.get_target_display_label(*target);
                SendMessageW(
                    target_combo, CB_ADDSTRING, None, LPARAM(HSTRING::from(label).as_ptr() as _)
                );
            }

            let default_target_enum = installer::Target::default();
            let default_target_idx = installer::Target::VALUES.iter().position(|&t| t == default_target_enum).unwrap_or(0);

            update_target(dialog, target_combo, default_target_idx);

            if installed_count <= 1 {
                let _ = EnableWindow(target_combo, false);
            }

            // Show notice if install dir is not detected
            if installer.install_dir.is_none() {
                MessageBoxW(
                    dialog,
                    w!("Failed to detect the game's install location. Please select it manually."),
                    w!("Warning"),
                    MB_ICONWARNING | MB_OK
                );
            }

            1
        },

        WM_COMMAND => {
            let control_id = wparam.0 as i16 as i32;
            let notif_code = wparam.0 as u32 >> 16;
            let control = HWND(lparam.0 as _);

            match control_id {
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
                                &HSTRING::from(format!("Hachimi is already installed as {}", target.dll_name())),
                                w!("Error"),
                                MB_ICONERROR | MB_OK
                            );
                            return 0;
                        }
                    }
                    if installer.is_current_target_installed() {
                        let res = MessageBoxW(
                            dialog,
                            &HSTRING::from(format!("Replace {}?", installer.target.dll_name())),
                            w!("Install"),
                            MB_ICONINFORMATION | MB_OKCANCEL
                        );
                        if res != IDOK {
                            return 0;
                        }
                    }
                    // failed pre install no longer catastrophic, just warn
                    if installer.pre_install().is_err() {
                        MessageBoxW(
                            dialog,
                            w!("Failed to back up game EXE, use caution when uninstalling."),
                            w!("Warning"),
                            MB_ICONWARNING | MB_OK
                        );
                    }
                    match installer.install()
                        .and_then(|_| installer.post_install())
                    {
                        Ok(_) => {
                            MessageBoxW(dialog, w!("Install completed. Be sure to reinstall if the game updates."), w!("Success"), MB_ICONINFORMATION | MB_OK);
                        },
                        Err(e) => {
                            MessageBoxW(dialog, &HSTRING::from(e.to_string()), w!("Error"), MB_ICONERROR | MB_OK);
                        }
                    }
                    update_target(dialog, GetDlgItem(dialog, IDC_TARGET).unwrap(), installer.target as _);
                }

                IDC_UNINSTALL => {
                    let installer = get_installer(dialog);
                    let res = MessageBoxW(
                        dialog,
                        &HSTRING::from(format!("Delete {}?", installer.target.dll_name())),
                        w!("Uninstall"),
                        MB_ICONINFORMATION | MB_OKCANCEL
                    );
                    if res == IDOK {
                        let version_info_opt = installer.get_target_version_info(installer.target);
                        if let Err(e) = installer.uninstall() {
                            // fall through but clarify danger
                            // only interrupt if error is not FailedToRestore
                            if matches!(e, installer::Error::FailedToRestore) {
                                MessageBoxW(dialog, &HSTRING::from(e.to_string()), w!("Warning"), MB_ICONWARNING | MB_OK);
                            } else {
                                MessageBoxW(dialog, &HSTRING::from(e.to_string()), w!("Error"), MB_ICONERROR | MB_OK);
                                return 0;
                            };
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
                                    w!("Do you also want to delete Hachimi's data directory?"),
                                    w!("Uninstall"),
                                    MB_ICONINFORMATION | MB_YESNO
                                );

                                if res == IDYES {
                                    if let Err(e) = std::fs::remove_dir_all(&hachimi_dir) {
                                        MessageBoxW(dialog, &HSTRING::from(e.to_string()), w!("Error"), MB_ICONERROR | MB_OK);
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