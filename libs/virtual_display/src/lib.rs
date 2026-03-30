//! 가상 디스플레이 드라이버 관리 라이브러리
//! Windows에서 가상 디스플레이를 생성하고 관리합니다.
use hbb_common::{anyhow, dlopen::symbor::Library, log, ResultType};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

/// 가상 디스플레이 동적 라이브러리 이름
const LIB_NAME_VIRTUAL_DISPLAY: &str = "dylib_virtual_display";

/// Windows DWORD 타입
pub type DWORD = ::std::os::raw::c_ulong;

/// 모니터 모드 (해상도 및 주사율)를 나타내는 구조체
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct _MonitorMode {
    /// 모니터 너비 (픽셀)
    pub width: DWORD,
    /// 모니터 높이 (픽셀)
    pub height: DWORD,
    /// 주사율 (Hz)
    pub sync: DWORD,
}
pub type MonitorMode = _MonitorMode;
pub type PMonitorMode = *mut MonitorMode;

/// 드라이버 설치 경로를 반환하는 함수 타입
pub type GetDriverInstallPath = fn() -> &'static str;
/// 장치가 생성되었는지 확인하는 함수 타입
pub type IsDeviceCreated = fn() -> bool;
/// 장치를 종료하는 함수 타입
pub type CloseDevice = fn();
/// 드라이버를 다운로드하는 함수 타입
pub type DownLoadDriver = fn() -> ResultType<()>;
/// 장치를 생성하는 함수 타입
pub type CreateDevice = fn() -> ResultType<()>;
/// 드라이버를 설치하거나 업데이트하는 함수 타입
pub type InstallUpdateDriver = fn(&mut bool) -> ResultType<()>;
/// 드라이버를 제거하는 함수 타입
pub type UninstallDriver = fn(&mut bool) -> ResultType<()>;
/// 모니터를 연결하는 함수 타입
pub type PlugInMonitor = fn(u32, u32, u32) -> ResultType<()>;
/// 모니터를 분리하는 함수 타입
pub type PlugOutMonitor = fn(u32) -> ResultType<()>;
/// 모니터 모드를 업데이트하는 함수 타입
pub type UpdateMonitorModes = fn(u32, u32, PMonitorMode) -> ResultType<()>;

macro_rules! make_lib_wrapper {
    ($($field:ident : $tp:ty),+) => {
        struct LibWrapper {
            _lib: Option<Library>,
            $($field: Option<$tp>),+
        }

        impl LibWrapper {
            fn new() -> Self {
                let lib = match Library::open(get_lib_name()) {
                    Ok(lib) => Some(lib),
                    Err(e) => {
                        log::warn!("Failed to load library {}, {}", LIB_NAME_VIRTUAL_DISPLAY, e);
                        None
                    }
                };

                $(let $field = if let Some(lib) = &lib {
                    match unsafe { lib.symbol::<$tp>(stringify!($field)) } {
                        Ok(m) => {
                            Some(*m)
                        },
                        Err(e) => {
                            log::warn!("Failed to load func {}, {}", stringify!($field), e);
                            None
                        }
                    }
                } else {
                    None
                };)+

                Self {
                    _lib: lib,
                    $( $field ),+
                }
            }
        }

        impl Default for LibWrapper {
            fn default() -> Self {
                Self::new()
            }
        }
    }
}

make_lib_wrapper!(
    get_driver_install_path: GetDriverInstallPath,
    is_device_created: IsDeviceCreated,
    close_device: CloseDevice,
    download_driver: DownLoadDriver,
    create_device: CreateDevice,
    install_update_driver: InstallUpdateDriver,
    uninstall_driver: UninstallDriver,
    plug_in_monitor: PlugInMonitor,
    plug_out_monitor: PlugOutMonitor,
    update_monitor_modes: UpdateMonitorModes
);

lazy_static::lazy_static! {
    static ref LIB_WRAPPER: Arc<Mutex<LibWrapper>> = Default::default();
    static ref MONITOR_INDICES: Mutex<HashSet<u32>> = Mutex::new(HashSet::new());
}

#[cfg(target_os = "windows")]
fn get_lib_name() -> String {
    format!("{}.dll", LIB_NAME_VIRTUAL_DISPLAY)
}

#[cfg(target_os = "linux")]
fn get_lib_name() -> String {
    format!("lib{}.so", LIB_NAME_VIRTUAL_DISPLAY)
}

#[cfg(target_os = "macos")]
fn get_lib_name() -> String {
    format!("lib{}.dylib", LIB_NAME_VIRTUAL_DISPLAY)
}

#[cfg(windows)]
pub fn get_driver_install_path() -> Option<&'static str> {
    Some(LIB_WRAPPER.lock().unwrap().get_driver_install_path?())
}

pub fn is_device_created() -> bool {
    LIB_WRAPPER
        .lock()
        .unwrap()
        .is_device_created
        .map(|f| f())
        .unwrap_or(false)
}

pub fn close_device() {
    let _r = LIB_WRAPPER.lock().unwrap().close_device.map(|f| f());
}

pub fn download_driver() -> ResultType<()> {
    LIB_WRAPPER
        .lock()
        .unwrap()
        .download_driver
        .ok_or(anyhow::Error::msg("download_driver method not found"))?()
}

pub fn create_device() -> ResultType<()> {
    LIB_WRAPPER
        .lock()
        .unwrap()
        .create_device
        .ok_or(anyhow::Error::msg("create_device method not found"))?()
}

pub fn install_update_driver(reboot_required: &mut bool) -> ResultType<()> {
    LIB_WRAPPER
        .lock()
        .unwrap()
        .install_update_driver
        .ok_or(anyhow::Error::msg("install_update_driver method not found"))?(reboot_required)
}

pub fn uninstall_driver(reboot_required: &mut bool) -> ResultType<()> {
    LIB_WRAPPER
        .lock()
        .unwrap()
        .uninstall_driver
        .ok_or(anyhow::Error::msg("uninstall_driver method not found"))?(reboot_required)
}

#[cfg(windows)]
pub fn plug_in_monitor(monitor_index: u32) -> ResultType<()> {
    let mut lock = MONITOR_INDICES.lock().unwrap();
    if lock.contains(&monitor_index) {
        return Ok(());
    }
    let f = LIB_WRAPPER
        .lock()
        .unwrap()
        .plug_in_monitor
        .ok_or(anyhow::Error::msg("plug_in_monitor method not found"))?;
    f(monitor_index, 0, 20)?;
    lock.insert(monitor_index);
    Ok(())
}

#[cfg(windows)]
pub fn plug_out_monitor(monitor_index: u32) -> ResultType<()> {
    let f = LIB_WRAPPER
        .lock()
        .unwrap()
        .plug_out_monitor
        .ok_or(anyhow::Error::msg("plug_out_monitor method not found"))?;
    f(monitor_index)?;
    MONITOR_INDICES.lock().unwrap().remove(&monitor_index);
    Ok(())
}

#[cfg(windows)]
pub fn update_monitor_modes(monitor_index: u32, modes: &[MonitorMode]) -> ResultType<()> {
    let f = LIB_WRAPPER
        .lock()
        .unwrap()
        .update_monitor_modes
        .ok_or(anyhow::Error::msg("update_monitor_modes method not found"))?;
    f(monitor_index, modes.len() as _, modes.as_ptr() as _)
}
