use once_cell::sync::Lazy;
use std::sync::Mutex;

rust_i18n::i18n!("locales");

pub use rust_i18n::t;

pub static CURRENT_LOCALE: Lazy<Mutex<String>> =
    Lazy::new(|| Mutex::new(rust_i18n::locale().to_string()));

pub fn set_locale(lang: &str) {
    rust_i18n::set_locale(lang);
    *CURRENT_LOCALE.lock().unwrap() = lang.to_string();
}
