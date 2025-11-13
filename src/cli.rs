use std::path::{Path, PathBuf};

use crate::i18n::{t};
use windows::{
    core::{w, HSTRING},
    Win32::UI::{
        Shell::ShellExecuteW,
        WindowsAndMessaging::{MessageBoxW, IDCANCEL, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_OKCANCEL, SW_NORMAL}
    }
};

use crate::{installer::{self, Installer, Target}, utils};

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

pub fn run() -> Result<bool, installer::Error> {
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
                        &HSTRING::from(t!("cli.game_running")),
                        &HSTRING::from(t!("cli.installer_title")),
                        MB_ICONINFORMATION | MB_OKCANCEL
                    );
                    if res == IDCANCEL {
                        return Ok(true);
                    }
                }
            }
        }

        if args.install_dir.is_none() {
            if let Some(target) = &args.target {
                // Check if target is an absolute path;
                // If it is, set the install dir unconditionally so that it will be completely
                // overridden by the target later (without relying on install dir detection)
                let target_path = Path::new(target);
                if target_path.is_absolute() {
                    // Doesn't matter which path it is, just use the target path
                    args.install_dir = Some(target_path.into());
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
                    &HSTRING::from(t!("cli.failed_determine_target")),
                    &HSTRING::from(t!("cli.installer_title")),
                    MB_ICONERROR | MB_OK
                );
            }
            std::process::exit(128);
        });

        let installer = Installer::custom(args.install_dir, explicit_target, args.target);
        let res = match command {
            Command::Install => {
                let mut res = Ok(());
                if args.pre_install {
                    res = res.and_then(|_| installer.pre_install());
                }
                res = res.and_then(|_| installer.install());
                if args.post_install {
                    res = res.and_then(|_| installer.post_install());
                }
                res
            },
            Command::Uninstall => installer.uninstall()
        };
        if let Err(e) = res {
            unsafe { MessageBoxW(None, &HSTRING::from(e.to_string()), w!("Hachimi Installer"), MB_ICONERROR | MB_OK); }
            return Err(e);
        }

        if args.launch_game {
            let game_dir = installer.install_dir.unwrap();
            let exe_path = game_dir.join("umamusume.exe");
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