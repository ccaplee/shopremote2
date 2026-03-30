use crate::ui_interface::get_option;
#[cfg(windows)]
use crate::{
    display_service,
    ipc::{connect, Data},
    platform::is_installed,
};
#[cfg(windows)]
use hbb_common::tokio;
use hbb_common::{anyhow::anyhow, bail, lazy_static, tokio::sync::oneshot, ResultType};
use serde_derive::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[cfg(windows)]
pub mod win_exclude_from_capture;
#[cfg(windows)]
mod win_input;
#[cfg(windows)]
pub mod win_mag;
#[cfg(windows)]
pub mod win_topmost_window;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(windows)]
mod win_virtual_display;
#[cfg(windows)]
pub use win_virtual_display::restore_reg_connectivity;

// 유효하지 않은 프라이버시 모드 연결 ID
pub const INVALID_PRIVACY_MODE_CONN_ID: i32 = 0;
// 프라이버시 모드 점유 오류 메시지
pub const OCCUPIED: &'static str = "Privacy occupied by another one.";
// 다른 연결의 프라이버시 모드 종료 오류 메시지
pub const TURN_OFF_OTHER_ID: &'static str =
    "Failed to turn off privacy mode that belongs to someone else.";
// 물리적 디스플레이 없음 메시지
pub const NO_PHYSICAL_DISPLAYS: &'static str = "no_need_privacy_mode_no_physical_displays_tip";

// Windows Magnifier 기반 프라이버시 모드 구현 키
pub const PRIVACY_MODE_IMPL_WIN_MAG: &str = "privacy_mode_impl_mag";
// Windows 화면 캡처 제외 기반 프라이버시 모드 구현 키
pub const PRIVACY_MODE_IMPL_WIN_EXCLUDE_FROM_CAPTURE: &str =
    "privacy_mode_impl_exclude_from_capture";
// Windows 가상 디스플레이 기반 프라이버시 모드 구현 키
pub const PRIVACY_MODE_IMPL_WIN_VIRTUAL_DISPLAY: &str = "privacy_mode_impl_virtual_display";

/// 프라이버시 모드 상태 표현
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "t", content = "c")]
pub enum PrivacyModeState {
    // 프라이버시 모드 종료 성공
    OffSucceeded,
    // 원격 사용자가 프라이버시 모드 종료
    OffByPeer,
    // 프라이버시 모드 상태 불명
    OffUnknown,
}

/// 프라이버시 모드 구현 인터페이스 트레이트
/// 다양한 프라이버시 모드 구현(MAG, ExcludeFromCapture, VirtualDisplay 등)을 정의
pub trait PrivacyMode: Sync + Send {
    /// 비동기 프라이버시 모드인지 확인
    fn is_async_privacy_mode(&self) -> bool;

    /// 프라이버시 모드 구현 초기화
    fn init(&self) -> ResultType<()>;
    /// 프라이버시 모드 구현 정리
    fn clear(&mut self);
    /// 프라이버시 모드 켜기 (true: 이미 켜져있음, false: 새로 켜짐)
    fn turn_on_privacy(&mut self, conn_id: i32) -> ResultType<bool>;
    /// 프라이버시 모드 끄기
    fn turn_off_privacy(&mut self, conn_id: i32, state: Option<PrivacyModeState>)
        -> ResultType<()>;

    /// 현재 프라이버시 모드 점유 연결 ID 반환
    fn pre_conn_id(&self) -> i32;

    /// 프라이버시 모드 구현 키 반환
    fn get_impl_key(&self) -> &str;

    /// 프라이버시 모드 켜기 전 연결 ID 확인
    /// 같은 연결이면 true, 새 연결이고 가능하면 false, 점유중이면 오류
    #[inline]
    fn check_on_conn_id(&self, conn_id: i32) -> ResultType<bool> {
        let pre_conn_id = self.pre_conn_id();
        if pre_conn_id == conn_id {
            return Ok(true);
        }
        if pre_conn_id != INVALID_PRIVACY_MODE_CONN_ID {
            bail!(OCCUPIED);
        }
        Ok(false)
    }

    /// 프라이버시 모드 끄기 전 연결 ID 확인
    /// 다른 연결이 점유중이면 오류 반환
    #[inline]
    fn check_off_conn_id(&self, conn_id: i32) -> ResultType<()> {
        let pre_conn_id = self.pre_conn_id();
        if pre_conn_id != INVALID_PRIVACY_MODE_CONN_ID
            && conn_id != INVALID_PRIVACY_MODE_CONN_ID
            && pre_conn_id != conn_id
        {
            bail!(TURN_OFF_OTHER_ID)
        }
        Ok(())
    }
}

lazy_static::lazy_static! {
    pub static ref DEFAULT_PRIVACY_MODE_IMPL: String = {
        #[cfg(windows)]
        {
            if win_exclude_from_capture::is_supported() {
                PRIVACY_MODE_IMPL_WIN_EXCLUDE_FROM_CAPTURE
            } else {
                if display_service::is_privacy_mode_mag_supported() {
                    PRIVACY_MODE_IMPL_WIN_MAG
                } else {
                    if is_installed() {
                        PRIVACY_MODE_IMPL_WIN_VIRTUAL_DISPLAY
                    } else {
                        ""
                    }
                }
            }.to_owned()
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            {
                macos::PRIVACY_MODE_IMPL.to_owned()
            }
            #[cfg(not(target_os = "macos"))]
            {
                "".to_owned()
            }
        }
    };

    static ref PRIVACY_MODE: Arc<Mutex<Option<Box<dyn PrivacyMode>>>> = {
        let mut cur_impl = get_option("privacy-mode-impl-key".to_owned());
        if !get_supported_privacy_mode_impl().iter().any(|(k, _)| k == &cur_impl) {
            cur_impl = DEFAULT_PRIVACY_MODE_IMPL.to_owned();
        }

        let privacy_mode = match PRIVACY_MODE_CREATOR.lock().unwrap().get(&(&cur_impl as &str)) {
            Some(creator) => Some(creator(&cur_impl)),
            None => None,
        };
        Arc::new(Mutex::new(privacy_mode))
    };
}

pub type PrivacyModeCreator = fn(impl_key: &str) -> Box<dyn PrivacyMode>;
lazy_static::lazy_static! {
    static ref PRIVACY_MODE_CREATOR: Arc<Mutex<HashMap<&'static str, PrivacyModeCreator>>> = {
        #[cfg(not(windows))]
        let mut map: HashMap<&'static str, PrivacyModeCreator> = HashMap::new();
        #[cfg(target_os = "macos")]
        {
            map.insert(macos::PRIVACY_MODE_IMPL, |impl_key: &str| {
                Box::new(macos::PrivacyModeImpl::new(impl_key))
            });
        }
        #[cfg(windows)]
        let mut map: HashMap<&'static str, PrivacyModeCreator> = HashMap::new();
        #[cfg(windows)]
        {
            if win_exclude_from_capture::is_supported() {
                map.insert(win_exclude_from_capture::PRIVACY_MODE_IMPL, |impl_key: &str| {
                    Box::new(win_exclude_from_capture::PrivacyModeImpl::new(impl_key))
                });
            } else {
                map.insert(win_mag::PRIVACY_MODE_IMPL, |impl_key: &str| {
                    Box::new(win_mag::PrivacyModeImpl::new(impl_key))
                });
            }

            map.insert(win_virtual_display::PRIVACY_MODE_IMPL, |impl_key: &str| {
                    Box::new(win_virtual_display::PrivacyModeImpl::new(impl_key))
                });
        }
        Arc::new(Mutex::new(map))
    };
}

/// 프라이버시 모드 구현 초기화
#[inline]
pub fn init() -> Option<ResultType<()>> {
    Some(PRIVACY_MODE.lock().unwrap().as_ref()?.init())
}

/// 프라이버시 모드 구현 정리
#[inline]
pub fn clear() -> Option<()> {
    Some(PRIVACY_MODE.lock().unwrap().as_mut()?.clear())
}

/// 프라이버시 모드 구현 전환
/// 다른 구현으로 변경하면 이전 구현 정리 후 새 구현 생성
#[inline]
pub fn switch(impl_key: &str) {
    let mut privacy_mode_lock = PRIVACY_MODE.lock().unwrap();
    if let Some(privacy_mode) = privacy_mode_lock.as_ref() {
        if privacy_mode.get_impl_key() == impl_key {
            return;
        }
    }

    if let Some(creator) = PRIVACY_MODE_CREATOR.lock().unwrap().get(impl_key) {
        *privacy_mode_lock = Some(creator(impl_key));
    }
}

fn get_supported_impl(impl_key: &str) -> String {
    let supported_impls = get_supported_privacy_mode_impl();
    if supported_impls.iter().any(|(k, _)| k == &impl_key) {
        return impl_key.to_owned();
    };
    // TODO: Is it a good idea to use fallback here? Because user do not know the fallback.
    // fallback
    let mut cur_impl = get_option("privacy-mode-impl-key".to_owned());
    if !get_supported_privacy_mode_impl()
        .iter()
        .any(|(k, _)| k == &cur_impl)
    {
        // fallback
        cur_impl = DEFAULT_PRIVACY_MODE_IMPL.to_owned();
    }
    cur_impl
}

/// 프라이버시 모드 켜기 (비동기/동기 모드 자동 선택)
/// impl_key: 사용할 프라이버시 모드 구현
/// conn_id: 연결 ID
/// true: 이미 켜져있음, false: 새로 켜짐
pub async fn turn_on_privacy(impl_key: &str, conn_id: i32) -> Option<ResultType<bool>> {
    if is_async_privacy_mode() {
        turn_on_privacy_async(impl_key.to_string(), conn_id).await
    } else {
        turn_on_privacy_sync(impl_key, conn_id)
    }
}

/// 현재 프라이버시 모드가 비동기인지 확인
#[inline]
fn is_async_privacy_mode() -> bool {
    PRIVACY_MODE
        .lock()
        .unwrap()
        .as_ref()
        .map_or(false, |m| m.is_async_privacy_mode())
}

/// 비동기 프라이버시 모드 켜기 (타임아웃 7.5초)
/// 별도 스레드에서 동기 작업 수행 후 결과 반환
#[inline]
async fn turn_on_privacy_async(impl_key: String, conn_id: i32) -> Option<ResultType<bool>> {
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let res = turn_on_privacy_sync(&impl_key, conn_id);
        let _ = tx.send(res);
    });
    // 최대 7.5초 대기
    // Amyuni IDD를 사용한 프라이버시 모드 켜기는 시간이 걸릴 수 있음
    // 일부 노트북은 가상 디스플레이 연결 시 시간 소요
    match hbb_common::timeout(7500, rx).await {
        Ok(res) => match res {
            Ok(res) => res,
            Err(e) => Some(Err(anyhow!(e.to_string()))),
        },
        Err(e) => Some(Err(anyhow!(e.to_string()))),
    }
}

/// 동기 프라이버시 모드 켜기
/// 프라이버시 모드 이미 켜졌는지 확인, 필요시 구현 전환
fn turn_on_privacy_sync(impl_key: &str, conn_id: i32) -> Option<ResultType<bool>> {
    // 프라이버시 모드 이미 켜져있거나 다른 연결이 점유중인지 확인
    let mut privacy_mode_lock = PRIVACY_MODE.lock().unwrap();

    // 지원되는 프라이버시 모드 구현 확인 또는 전환
    let impl_key = get_supported_impl(impl_key);

    let mut cur_impl_key = "".to_string();
    if let Some(privacy_mode) = privacy_mode_lock.as_ref() {
        cur_impl_key = privacy_mode.get_impl_key().to_string();
        let check_on_conn_id = privacy_mode.check_on_conn_id(conn_id);
        match check_on_conn_id.as_ref() {
            Ok(true) => {
                if cur_impl_key == impl_key {
                    // Same peer, same implementation.
                    return Some(Ok(true));
                } else {
                    // Same peer, switch to new implementation.
                }
            }
            Err(_) => return Some(check_on_conn_id),
            _ => {}
        }
    }

    if cur_impl_key != impl_key {
        if let Some(creator) = PRIVACY_MODE_CREATOR
            .lock()
            .unwrap()
            .get(&(&impl_key as &str))
        {
            if let Some(privacy_mode) = privacy_mode_lock.as_mut() {
                privacy_mode.clear();
            }

            *privacy_mode_lock = Some(creator(&impl_key));
        } else {
            return Some(Err(anyhow!("Unsupported privacy mode: {}", impl_key)));
        }
    }

    // turn on privacy mode
    Some(privacy_mode_lock.as_mut()?.turn_on_privacy(conn_id))
}

#[inline]
pub fn turn_off_privacy(conn_id: i32, state: Option<PrivacyModeState>) -> Option<ResultType<()>> {
    Some(
        PRIVACY_MODE
            .lock()
            .unwrap()
            .as_mut()?
            .turn_off_privacy(conn_id, state),
    )
}

#[inline]
pub fn check_on_conn_id(conn_id: i32) -> Option<ResultType<bool>> {
    Some(
        PRIVACY_MODE
            .lock()
            .unwrap()
            .as_ref()?
            .check_on_conn_id(conn_id),
    )
}

#[cfg(windows)]
#[tokio::main(flavor = "current_thread")]
async fn set_privacy_mode_state(
    conn_id: i32,
    state: PrivacyModeState,
    impl_key: String,
    ms_timeout: u64,
) -> ResultType<()> {
    let mut c = connect(ms_timeout, "_cm").await?;
    c.send(&Data::PrivacyModeState((conn_id, state, impl_key)))
        .await
}

/// 지원되는 프라이버시 모드 구현 목록 반환
/// (구현 키, 설명 메시지 키) 튜플의 벡터
pub fn get_supported_privacy_mode_impl() -> Vec<(&'static str, &'static str)> {
    #[cfg(target_os = "windows")]
    {
        let mut vec_impls = Vec::new();

        // ExcludeFromCapture 지원 (Windows 10 2004+)
        if win_exclude_from_capture::is_supported() {
            vec_impls.push((
                PRIVACY_MODE_IMPL_WIN_EXCLUDE_FROM_CAPTURE,
                "privacy_mode_impl_mag_tip",
            ));
        } else {
            // Windows Magnifier 지원
            if display_service::is_privacy_mode_mag_supported() {
                vec_impls.push((PRIVACY_MODE_IMPL_WIN_MAG, "privacy_mode_impl_mag_tip"));
            }
        }

        // 가상 디스플레이 지원 (RustDesk 서비스 설치 필요)
        if is_installed() && crate::platform::windows::is_self_service_running() {
            vec_impls.push((
                PRIVACY_MODE_IMPL_WIN_VIRTUAL_DISPLAY,
                "privacy_mode_impl_virtual_display_tip",
            ));
        }

        vec_impls
    }
    #[cfg(target_os = "macos")]
    {
        // No translation is intended for privacy_mode_impl_macos_tip as it is a 
        // placeholder for macOS specific privacy mode implementation which currently
        // doesn't provide multiple modes like Windows does.
        vec![(macos::PRIVACY_MODE_IMPL, "privacy_mode_impl_macos_tip")]
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Vec::new()
    }
}

#[inline]
pub fn get_cur_impl_key() -> Option<String> {
    PRIVACY_MODE
        .lock()
        .unwrap()
        .as_ref()
        .map(|pm| pm.get_impl_key().to_owned())
}

#[inline]
pub fn is_current_privacy_mode_impl(impl_key: &str) -> bool {
    PRIVACY_MODE
        .lock()
        .unwrap()
        .as_ref()
        .map(|pm| pm.get_impl_key() == impl_key)
        .unwrap_or(false)
}

#[inline]
#[cfg(not(windows))]
pub fn check_privacy_mode_err(
    _privacy_mode_id: i32,
    _display_idx: usize,
    _timeout_millis: u64,
) -> String {
    "".to_owned()
}

#[inline]
#[cfg(windows)]
pub fn check_privacy_mode_err(
    privacy_mode_id: i32,
    display_idx: usize,
    timeout_millis: u64,
) -> String {
    // win magnifier implementation requires a test of creating a capturer.
    if is_current_privacy_mode_impl(PRIVACY_MODE_IMPL_WIN_MAG) {
        crate::video_service::test_create_capturer(privacy_mode_id, display_idx, timeout_millis)
    } else {
        "".to_owned()
    }
}

#[inline]
pub fn is_privacy_mode_supported() -> bool {
    !DEFAULT_PRIVACY_MODE_IMPL.is_empty()
}

#[inline]
pub fn get_privacy_mode_conn_id() -> Option<i32> {
    PRIVACY_MODE
        .lock()
        .unwrap()
        .as_ref()
        .map(|pm| pm.pre_conn_id())
}

#[inline]
pub fn is_in_privacy_mode() -> bool {
    PRIVACY_MODE
        .lock()
        .unwrap()
        .as_ref()
        .map(|pm| pm.pre_conn_id() != INVALID_PRIVACY_MODE_CONN_ID)
        .unwrap_or(false)
}
