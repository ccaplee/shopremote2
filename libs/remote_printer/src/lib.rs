//! 원격 프린터 설정 및 관리 라이브러리
//! Windows 운영 체제에서 원격 프린터 드라이버를 설치하고 관리합니다.

#[cfg(target_os = "windows")]
mod setup;
#[cfg(target_os = "windows")]
pub use setup::{
    is_rd_printer_installed,
    setup::{install_update_printer, uninstall_printer},
};

/// 원격 프린터 드라이버 INF 파일 경로
#[cfg(target_os = "windows")]
const RD_DRIVER_INF_PATH: &str = "drivers/ShopRemote2PrinterDriver/ShopRemote2PrinterDriver.inf";

/// 프린터 이름을 UTF-16 인코딩으로 반환합니다.
#[cfg(target_os = "windows")]
fn get_printer_name(app_name: &str) -> Vec<u16> {
    format!("{} Printer", app_name)
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

/// 프린터 드라이버 이름을 UTF-16 인코딩으로 반환합니다.
#[cfg(target_os = "windows")]
fn get_driver_name() -> Vec<u16> {
    "ShopRemote2 v4 Printer Driver"
        .encode_utf16()
        .chain(Some(0))
        .collect()
}

/// 프린터 포트 이름을 UTF-16 인코딩으로 반환합니다.
#[cfg(target_os = "windows")]
fn get_port_name(app_name: &str) -> Vec<u16> {
    format!("{} Printer", app_name)
        .encode_utf16()
        .chain(Some(0))
        .collect()
}
