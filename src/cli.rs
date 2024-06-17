use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use windows::{core::w, Win32::UI::WindowsAndMessaging::{MessageBoxW, IDCANCEL, MB_ICONINFORMATION, MB_OKCANCEL}};

use crate::{installer::{self, Installer}, utils};

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(long)]
    install_dir: Option<PathBuf>,

    #[arg(long)]
    target: Option<String>,

    #[arg(long)]
    sleep: Option<u64>,

    #[arg(long, action)]
    prompt_for_game_exit: bool
}

#[derive(Subcommand)]
enum Commands {
    Install,
    Uninstall
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
        match command {
            Commands::Install => installer.install()?,
            Commands::Uninstall => installer.uninstall()?
        }

        Ok(true)
    }
    else {
        Ok(false)
    }
}