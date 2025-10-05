#![windows_subsystem = "windows"]

mod installer;
mod resource;
mod utils;
mod cli;
mod gui;
mod updater;

#[cfg(feature = "compress_dll")]
#[macro_use]
extern crate include_bytes_zstd;

fn main() -> Result<(), installer::Error> {
    let update_status = updater::run_update_check();


    // Command line interface / Unattended mode
    if cli::run(&update_status)? {
        return Ok(());
    }

    // GUI mode (no arguments)
    if let Err(e) = gui::run(update_status) {
        e.code().unwrap();
    }

    Ok(())
}
