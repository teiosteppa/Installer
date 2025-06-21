#![windows_subsystem = "windows"]

mod i18n;
mod installer;
mod resource;
mod utils;
mod cli;
mod gui;

pub use crate::i18n::_rust_i18n_translate;

#[cfg(feature = "compress_dll")]
#[macro_use]
extern crate include_bytes_zstd;

fn main() -> Result<(), installer::Error> {
    // Set language by system language
    i18n::init_locale();

    // Command line interface / Unattended mode
    if cli::run()? { return Ok(()); }

    // GUI mode (no arguments)
    if let Err(e) = gui::run() {
        e.code().unwrap();
    }

    Ok(())
}
