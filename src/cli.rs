use std::path::{Path, PathBuf};

use windows::{
    core::{w, HSTRING},
    Win32::UI::{
        Shell::ShellExecuteW,
        WindowsAndMessaging::{MessageBoxW, IDCANCEL, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_OKCANCEL, SW_NORMAL}
    }
};

use crate::{installer::{self, Installer, Target}, updater::UpdateStatus, utils};

#[derive(Default)]
struct Args {
    command: Option<Command>,
    install_dir: Option<PathBuf>,
    target: Option<String>,
    explicit_target: Option<Target>,
    sleep: Option<u64>,
    prompt_for_game_exit: bool,
    launch_game: bool,
    game_args: Vec<String>,
    pre_install: bool,
    post_install: bool
}

enum Command {
    Install,
    Uninstall
}

#[inline]
fn require_next_arg(args: &mut std::env::Args) -> String {
    args.next().unwrap_or_else(|| std::process::exit(128))
}

impl Args {
    fn parse() -> Args {
        let mut args = Args::default();

        let mut iter = std::env::args();
        iter.next();

        let mut in_game_args = false;
        loop {
            let Some(arg) = iter.next() else {
                break;
            };

            if in_game_args {
                args.game_args.push(arg);
                continue;
            }

            match arg.as_str() {
                "install" => args.command = Some(Command::Install),
                "uninstall" => args.command = Some(Command::Uninstall),


                "--install-dir" => args.install_dir = Some(require_next_arg(&mut iter).into()),
                "--target" => args.target = Some(require_next_arg(&mut iter)),
                "--explicit-target" => {
                    let dll_name = require_next_arg(&mut iter);
                    args.explicit_target = Some(*Target::VALUES.iter()
                        .filter(|t| t.dll_name() == dll_name)
                        .next()
                        .unwrap_or_else(|| std::process::exit(128))
                    );
                },
                "--sleep" => args.sleep = Some(require_next_arg(&mut iter).parse().unwrap_or_else(|_| std::process::exit(128))),
                "--prompt-for-game-exit" => args.prompt_for_game_exit = true,
                "--launch-game" => args.launch_game = true,
                "--pre-install" => args.pre_install = true,
                "--post-install" => args.post_install = true,
                "--" => in_game_args = true,

                _ => {
                    // Invalid argument
                    std::process::exit(128);
                }
            }
        }

        args
    }
}

pub fn run(update_status: &UpdateStatus) -> Result<bool, installer::Error> {
    match update_status {
        UpdateStatus::Updated(version) => println!(
            "[UPDATE] Successfully updated to the nightly build from {}! Please restart.",
            version
        ),
        UpdateStatus::NotNeeded => {
            println!("[UPDATE] You are already on the latest nightly build.")
        }
        UpdateStatus::Failed(msg) => eprintln!("[UPDATE ERROR] {}", msg),
        UpdateStatus::Disabled => {}
    }

    let mut args = Args::parse();
    
    if let Some(command) = args.command {
        if let Some(sleep) = args.sleep {
            std::thread::sleep(std::time::Duration::from_millis(sleep));
        }

        if args.prompt_for_game_exit {
            while utils::is_game_running() {
                unsafe {
                    let res = MessageBoxW(
                        None,
                        w!("The game is currently running. Please close the game and press OK to install."),
                        w!("Hachimi Installer"),
                        MB_ICONINFORMATION | MB_OKCANCEL
                    );
                    if res == IDCANCEL {
                        return Ok(true);
                    }
                }
            }
        }

        let explicit_target = args.explicit_target.or_else(|| {
            let target_name = Path::new(args.target.as_ref()?).file_name()?;
            let target_name_str = target_name.to_string_lossy().to_ascii_lowercase();
            for t in Target::VALUES {
                if t.dll_name().to_ascii_lowercase() == target_name_str {
                    return Some(*t);
                }
            }
            None
        }).unwrap_or_else(|| {
            unsafe {
                MessageBoxW(
                    None,
                    w!("Failed to determine target type. Please make sure that the path is correct \
                        or explicitly specify a target name."),
                    w!("Hachimi Installer"),
                    MB_ICONERROR | MB_OK
                );
            }
            std::process::exit(128);
        });

        let mut installer = Installer::new(explicit_target, args.target);

        if let Some(dir) = args.install_dir {
            if let Err(e) = installer.set_install_dir(dir) {
                unsafe { MessageBoxW(None, &HSTRING::from(e.to_string()), w!("Hachimi Installer"), MB_ICONERROR | MB_OK); }
                return Err(e);
            }
        } else {
            installer.detect_install_dir();
        }

        let res: Result<(), installer::Error> = (|| {
            match command {
                Command::Install => {
                    if args.pre_install {
                        installer.pre_install()?;
                    }
                    installer.install()?;
                    if args.post_install {
                        installer.post_install()?;
                    }
                },
                Command::Uninstall => {
                    installer.uninstall()?;
                }
            }
            Ok(())
        })();

        if let Err(e) = res {
            unsafe { MessageBoxW(None, &HSTRING::from(e.to_string()), w!("Hachimi Installer"), MB_ICONERROR | MB_OK); }
            return Err(e);
        }

        if args.launch_game {
            let game_dir = installer.install_dir().unwrap();
            let exe_path = if game_dir.join("UmamusumePrettyDerby_Jpn.exe").is_file() {
                game_dir.join("UmamusumePrettyDerby_Jpn.exe")
            } else {
                game_dir.join("umamusume.exe")
            };
            unsafe {
                ShellExecuteW(
                    None,
                    None,
                    &HSTRING::from(exe_path.to_str().unwrap()),
                    &HSTRING::from(args.game_args.join(" ")),
                    &HSTRING::from(game_dir.to_str().unwrap()),
                    SW_NORMAL
                );
            }
        }

        Ok(true)
    }
    else {
        Ok(false)
    }
}