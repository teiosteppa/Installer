use once_cell::sync::Lazy;
use windows::Win32::Globalization::GetUserDefaultLocaleName;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

rust_i18n::i18n!("locales");

pub use rust_i18n::t;

pub const SUPPORTED_LOCALES: &[(&str, &str, &str)] = &[
    ("en-US", "en-US", "English"),
    ("zh-CN", "zh-CN", "简体中文"),
    ("zh-TW", "zh-TW", "繁體中文"),
];

pub static CURRENT_LOCALE: Lazy<std::sync::Mutex<String>> =
    Lazy::new(|| std::sync::Mutex::new(String::new()));

pub fn set_locale(lang: &str) {
    rust_i18n::set_locale(lang);
    *CURRENT_LOCALE.lock().unwrap() = lang.to_string();
}

pub fn init_locale() {
    if let Ok(lang) = std::env::var("HACHIMI_LANG") {
        set_locale(&lang); return;
    }

    let mut buf = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(&mut buf) } as usize; // 返回包含 '\0'
    let win_locale = if len > 1 {
        OsString::from_wide(&buf[..len - 1]).to_string_lossy().into_owned()
    } else {
        String::new()
    };
    let _win_locale = if len > 0 {
        OsString::from_wide(&buf[..len as usize - 1]).to_string_lossy().into_owned()
    } else {
        String::new()
    };

    let code = SUPPORTED_LOCALES
        .iter()
        .find(|(_, prefix, _)| win_locale.starts_with(*prefix))
        .map(|(code, ..)| *code)
        .unwrap_or("en-US");

    set_locale(code);
}