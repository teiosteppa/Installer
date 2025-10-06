#![windows_subsystem = "windows"]

mod installer;
mod resource;
mod utils;
mod cli;
mod gui;

#[cfg(feature = "compress_bin")]
#[macro_use]
extern crate include_bytes_zstd;

fn main() -> Result<(), installer::Error> {
    // Command line interface / Unattended mode
    if cli::run()? { return Ok(()); }

    // GUI mode (no arguments)
    if let Err(e) = gui::run() {
        e.code().unwrap();
    }

    Ok(())
}
