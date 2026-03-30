#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(windows)]
pub use windows::*;

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub mod win_device;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub mod delegate;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "linux")]
pub mod linux_desktop_manager;

#[cfg(target_os = "linux")]
pub mod gtk_sudo;

#[cfg(all(
    not(all(target_os = "windows", not(target_pointer_width = "64"))),
    not(any(target_os = "android", target_os = "ios"))
))]
use hbb_common::sysinfo::System;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use hbb_common::{message_proto::CursorData, sysinfo::Pid, ResultType};
use std::sync::{Arc, Mutex};
#[cfg(not(any(target_os = "macos", target_os = "android", target_os = "ios")))]
pub const SERVICE_INTERVAL: u64 = 300;

lazy_static::lazy_static! {
    static ref INSTALLING_SERVICE: Arc<Mutex<bool>>= Default::default();
}

pub fn installing_service() -> bool {
    INSTALLING_SERVICE.lock().unwrap().clone()
}

pub fn is_xfce() -> bool {
    #[cfg(target_os = "linux")]
    {
        return std::env::var_os("XDG_CURRENT_DESKTOP") == Some(std::ffi::OsString::from("XFCE"));
    }
    #[cfg(not(target_os = "linux"))]
    {
        return false;
    }
}

pub fn breakdown_callback() {
    #[cfg(target_os = "linux")]
    crate::input_service::clear_remapped_keycode();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    crate::input_service::release_device_modifiers();
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
/// 디스플레이 해상도를 변경하는 함수
/// 현재 해상도와 요청된 해상도를 비교하여 같으면 즉시 반환,
/// 다르면 플랫폼별 해상도 변경 함수를 호출함
/// macOS: 비교 로직이 올바르게 작동하는지 확인 필요
/// Linux: xrandr 명령 실행, DPI는 고려 대상 아님
/// Windows: dmPelsWidth/dmPelsHeight는 width/height와 동일
///         (이 프로세스가 DPI 인식 모드로 실행 중이므로)
pub fn change_resolution(name: &str, width: usize, height: usize) -> ResultType<()> {
    let cur_resolution = current_resolution(name)?;
    // macOS용 처리
    // 해결 필요: 다음의 비교 로직이 제대로 작동하는지 확인
    // Linux용 처리
    // "xrandr"을 실행하기만 함, DPI는 고려 대상이 아님
    // Windows용 처리
    // dmPelsWidth와 dmPelsHeight는 width와 height와 동일한 값
    // 이 프로세스가 DPI 인식 모드로 실행 중이기 때문
    if cur_resolution.width as usize == width && cur_resolution.height as usize == height {
        return Ok(());
    }
    hbb_common::log::warn!("Change resolution of '{}' to ({},{})", name, width, height);
    change_resolution_directly(name, width, height)
}

// Android용 활성 사용자명을 가져오는 함수 (미구현)
#[cfg(target_os = "android")]
pub fn get_active_username() -> String {
    // TODO: Android 사용자명 조회 구현 필요
    "android".into()
}

// Android의 포터블 오디오 샘플 레이트 (48kHz)
#[cfg(target_os = "android")]
pub const PA_SAMPLE_RATE: u32 = 48000;

/// Android 플랫폼에서 시스템 웨이크락(WakeLock)을 관리하는 구조체
/// 기기가 잠자기 모드로 진입하는 것을 방지
#[cfg(target_os = "android")]
#[derive(Default)]
pub struct WakeLock(Option<android_wakelock::WakeLock>);

// Android WakeLock 구현
#[cfg(target_os = "android")]
impl WakeLock {
    /// 주어진 태그를 사용하여 새로운 웨이크락 생성
    /// 앱 이름을 자동으로 태그 앞에 붙임
    pub fn new(tag: &str) -> Self {
        let tag = format!("{}:{tag}", crate::get_app_name());
        // partial() 웨이크락: CPU는 깨워두지만 디스플레이는 꺼질 수 있음
        match android_wakelock::partial(tag) {
            Ok(lock) => Self(Some(lock)),
            Err(e) => {
                hbb_common::log::error!("Failed to get wakelock: {e:?}");
                Self::default()
            }
        }
    }
}

/// 플랫폼별 웨이크락 생성 함수 (iOS 제외)
/// display 파라미터로 디스플레이 유지 여부 지정
#[cfg(not(target_os = "ios"))]
pub fn get_wakelock(_display: bool) -> WakeLock {
    hbb_common::log::info!("new wakelock, require display on: {_display}");
    #[cfg(target_os = "android")]
    return crate::platform::WakeLock::new("server");
    // display: 화면을 켜진 상태로 유지
    // idle: CPU를 깨워서 유지
    // sleep: 수동으로도 잠자기 모드로 진입하지 않도록 방지
    #[cfg(not(target_os = "android"))]
    return crate::platform::WakeLock::new(_display, true, false);
}

/// Windows/Linux에서 서비스 설치 상태를 관리하는 구조체
/// 이 구조체의 생성자를 사용하여 InstallingService 인스턴스 생성
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub(crate) struct InstallingService;

// InstallingService 구현
#[cfg(any(target_os = "windows", target_os = "linux"))]
impl InstallingService {
    /// 새로운 InstallingService 생성
    /// 생성 시 INSTALLING_SERVICE 플래그를 true로 설정
    pub fn new() -> Self {
        *INSTALLING_SERVICE.lock().unwrap() = true;
        Self
    }
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
impl Drop for InstallingService {
    fn drop(&mut self) {
        *INSTALLING_SERVICE.lock().unwrap() = false;
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
#[inline]
pub fn is_prelogin() -> bool {
    false
}

/// 주어진 이름과 인자를 가진 프로세스의 PID를 검색하는 내부 함수
/// 주의: Windows에서는 비효율적 - 모든 프로세스를 가져옴
/// 성능이 중요하지 않은 경우에만 호출되어야 함
/// 프로세스의 커맨드 라인을 직접 가져오려면 많은 추가 코드가 필요함
#[allow(dead_code)]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn get_pids_of_process_with_args<S1: AsRef<str>, S2: AsRef<str>>(
    name: S1,
    args: &[S2],
) -> Vec<Pid> {
    // 32비트 프로세스가 64비트 Windows에서 실행 중일 때는 이 함수가 작동하지 않음
    // process.cmd()가 항상 빈 배열을 반환하므로
    // 대신 windows::get_pids_with_args_by_wmic() 사용
    #[cfg(all(target_os = "windows", not(target_pointer_width = "64")))]
    {
        return windows::get_pids_with_args_by_wmic(name, args);
    }
    #[cfg(not(all(target_os = "windows", not(target_pointer_width = "64"))))]
    {
        let name = name.as_ref().to_lowercase();
        let system = System::new_all();
        system
            .processes()
            .iter()
            .filter(|(_, process)| {
                process.name().to_lowercase() == name
                    && process.cmd().len() == args.len() + 1
                    && args.iter().enumerate().all(|(i, arg)| {
                        process.cmd()[i + 1].to_lowercase() == arg.as_ref().to_lowercase()
                    })
            })
            .map(|(&pid, _)| pid)
            .collect()
    }
}

/// 주어진 이름과 첫 번째 인자를 가진 프로세스의 PID를 검색
/// 주의: Windows에서는 비효율적 - 모든 프로세스를 가져옴
/// 성능이 중요하지 않은 경우에만 호출되어야 함
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn get_pids_of_process_with_first_arg<S1: AsRef<str>, S2: AsRef<str>>(
    name: S1,
    arg: S2,
) -> Vec<Pid> {
    // 32비트 프로세스가 64비트 Windows에서 실행 중일 때는 이 함수가 작동하지 않음
    // process.cmd()가 항상 빈 배열을 반환하므로
    // 대신 windows::get_pids_with_first_arg_by_wmic() 사용
    #[cfg(all(target_os = "windows", not(target_pointer_width = "64")))]
    {
        return windows::get_pids_with_first_arg_by_wmic(name, arg);
    }
    #[cfg(not(all(target_os = "windows", not(target_pointer_width = "64"))))]
    {
        let name = name.as_ref().to_lowercase();
        let system = System::new_all();
        system
            .processes()
            .iter()
            .filter(|(_, process)| {
                process.name().to_lowercase() == name
                    && process.cmd().len() >= 2
                    && process.cmd()[1].to_lowercase() == arg.as_ref().to_lowercase()
            })
            .map(|(&pid, _)| pid)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_cursor_data() {
        for _ in 0..30 {
            if let Some(hc) = get_cursor().unwrap() {
                let cd = get_cursor_data(hc).unwrap();
                repng::encode(
                    std::fs::File::create("cursor.png").unwrap(),
                    cd.width as _,
                    cd.height as _,
                    &cd.colors[..],
                )
                .unwrap();
            }
            #[cfg(target_os = "macos")]
            macos::is_process_trusted(false);
        }
    }
    #[test]
    fn test_get_cursor_pos() {
        for _ in 0..30 {
            assert!(!get_cursor_pos().is_none());
        }
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn test_resolution() {
        let name = r"\\.\DISPLAY1";
        println!("current:{:?}", current_resolution(name));
        println!("change:{:?}", change_resolution(name, 2880, 1800));
        println!("resolutions:{:?}", resolutions(name));
    }
}
