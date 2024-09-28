use std::path::{Path, PathBuf};

use windows::{
    core::{w, HSTRING},
    Win32::UI::{
        Shell::ShellExecuteW,
        WindowsAndMessaging::{MessageBoxW, IDCANCEL, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_OKCANCEL, SW_NORMAL}
    }
};

use crate::{installer::{self, Installer}, utils};

#[derive(Default)]
struct Args {
    command: Option<Command>,
    install_dir: Option<PathBuf>,
    target: Option<String>,
    sleep: Option<u64>,
    prompt_for_game_exit: bool,
    launch_game: bool,
    game_args: Vec<String>
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
                "--sleep" => args.sleep = Some(require_next_arg(&mut iter).parse().unwrap_or_else(|_| std::process::exit(128))),
                "--prompt-for-game-exit" => args.prompt_for_game_exit = true,
                "--launch-game" => args.launch_game = true,
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

        let installer = Installer::custom(args.install_dir, args.target);
        let res = match command {
            Command::Install => installer.install(),
            Command::Uninstall => installer.uninstall()
        };
        if let Err(e) = res {
            unsafe { MessageBoxW(None, &HSTRING::from(e.to_string()), w!("Hachimi Installer"), MB_ICONERROR | MB_OK); }
            return Err(e);
        }

        if args.launch_game {
            let target_path = installer.get_current_target_path().unwrap();
            let game_dir = target_path.parent().unwrap();
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