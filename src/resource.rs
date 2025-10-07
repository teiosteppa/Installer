use windows::core::PCWSTR;

macro_rules! define_id {
    ($name:ident, $value:tt) => (
        pub const $name: PCWSTR = PCWSTR($value as u16 as *const u16);
    )
}

macro_rules! define_idc {
    ($name:ident, $value:tt) => (
        pub const $name: i32 = $value;
    )
}

// Dialogs
define_id!(IDD_MAIN, 129);

// Controls
define_idc!(IDC_INSTALL, 1000);
define_idc!(IDC_UNINSTALL, 1001);
define_idc!(IDC_PACKAGED_VER, 1002);
define_idc!(IDC_INSTALL_PATH, 1003);
define_idc!(IDC_INSTALL_PATH_BROWSE, 1004);
define_idc!(IDC_TARGET, 1005);
define_idc!(IDC_INSTALLED, 1006);
define_idc!(IDC_VERSION_STEAM, 1007);
define_idc!(IDC_VERSION_DMM, 1008);
define_idc!(IDC_VERSION_GROUP, 1009);

// Icons
define_id!(IDI_HACHIMI, 107);