// 플러그인 시스템 모듈
// 플러그인 로딩, 관리, 이벤트 처리 담당

use hbb_common::{bail, libc, log, ResultType};
#[cfg(target_os = "windows")]
use std::env;
use std::{
    ffi::{c_char, c_int, c_void, CStr},
    path::PathBuf,
    ptr::null,
};

// 플러그인 관련 하위 모듈
mod callback_ext;      // 콜백 확장 기능
mod callback_msg;      // 콜백 메시지 정의
mod config;            // 플러그인 설정 관리
pub mod desc;          // 플러그인 설명자
mod errno;             // 에러 코드 정의
pub mod ipc;           // 프로세스 간 통신
mod manager;           // 플러그인 생명주기 관리
pub mod native;        // 네이티브 플러그인 함수
pub mod native_handlers; // 네이티브 함수 핸들러
mod plog;              // 플러그인 로깅
mod plugins;           // 플러그인 인스턴스 관리

// 공개 API 내보내기
pub use manager::{
    install::{change_uninstall_plugin, install_plugin_with_url},
    install_plugin, load_plugin_list, remove_uninstalled, uninstall_plugin,
};
pub use plugins::{
    handle_client_event, handle_listen_event, handle_server_event, handle_ui_event, load_plugin,
    reload_plugin, sync_ui, unload_plugin,
};

// UI로 전송되는 메시지 타입 상수들
const MSG_TO_UI_TYPE_PLUGIN_EVENT: &str = "plugin_event";     // 플러그인 이벤트
const MSG_TO_UI_TYPE_PLUGIN_RELOAD: &str = "plugin_reload";   // 플러그인 재로드
const MSG_TO_UI_TYPE_PLUGIN_OPTION: &str = "plugin_option";   // 플러그인 옵션
const MSG_TO_UI_TYPE_PLUGIN_MANAGER: &str = "plugin_manager"; // 플러그인 관리자

// 플러그인 이벤트 상수들
pub const EVENT_ON_CONN_CLIENT: &str = "on_conn_client";             // 클라이언트 연결 시
pub const EVENT_ON_CONN_SERVER: &str = "on_conn_server";             // 서버 연결 시
pub const EVENT_ON_CONN_CLOSE_CLIENT: &str = "on_conn_close_client"; // 클라이언트 연결 종료 시
pub const EVENT_ON_CONN_CLOSE_SERVER: &str = "on_conn_close_server"; // 서버 연결 종료 시

// 로컬 플러그인 디렉토리
static PLUGIN_SOURCE_LOCAL_DIR: &str = "plugins";

// 설정 모듈 공개 내보내기
pub use config::{ManagerConfig, PeerConfig, SharedConfig};

/// 플러그인 공통 반환 값 구조체
///
/// 주의사항:
/// - code가 errno::ERR_SUCCESS이면 msg는 nullptr이어야 함
/// - code가 errno::ERR_SUCCESS가 아니면 msg는 호출자가 해제해야 함
#[repr(C)]
#[derive(Debug)]
pub struct PluginReturn {
    /// 반환 코드
    pub code: c_int,
    /// 에러 메시지 (C 문자열)
    pub msg: *const c_char,
}

/// PluginReturn 구현
impl PluginReturn {
    /// 성공 반환값 생성
    pub fn success() -> Self {
        Self {
            code: errno::ERR_SUCCESS,
            msg: null(),
        }
    }

    /// 성공 여부 확인
    #[inline]
    pub fn is_success(&self) -> bool {
        self.code == errno::ERR_SUCCESS
    }

    /// 에러 코드와 메시지로 반환값 생성
    pub fn new(code: c_int, msg: &str) -> Self {
        Self {
            code,
            msg: str_to_cstr_ret(msg),
        }
    }

    /// 코드와 메시지를 튜플로 추출
    pub fn get_code_msg(&mut self, id: &str) -> (i32, String) {
        if self.is_success() {
            (self.code, "".to_owned())
        } else {
            if self.msg.is_null() {
                log::warn!(
                    "The message pointer from the plugin '{}' is null, but the error code is {}",
                    id,
                    self.code
                );
                return (self.code, "".to_owned());
            }
            let msg = cstr_to_string(self.msg).unwrap_or_default();
            free_c_ptr(self.msg as _);
            self.msg = null();
            (self.code as _, msg)
        }
    }
}

fn is_server_running() -> bool {
    crate::common::is_server() || crate::common::is_server_running()
}

pub fn init() {
    if !is_server_running() {
        std::thread::spawn(move || manager::start_ipc());
    } else {
        if let Err(e) = remove_uninstalled() {
            log::error!("Failed to remove plugins: {}", e);
        }
    }
    match manager::get_uninstall_id_set() {
        Ok(ids) => {
            if let Err(e) = plugins::load_plugins(&ids) {
                log::error!("Failed to load plugins: {}", e);
            }
        }
        Err(e) => {
            log::error!("Failed to load plugins: {}", e);
        }
    }
}

#[inline]
#[cfg(target_os = "windows")]
fn get_share_dir() -> ResultType<PathBuf> {
    Ok(PathBuf::from(env::var("ProgramData")?))
}

#[inline]
#[cfg(target_os = "linux")]
fn get_share_dir() -> ResultType<PathBuf> {
    Ok(PathBuf::from("/usr/share"))
}

#[inline]
#[cfg(target_os = "macos")]
fn get_share_dir() -> ResultType<PathBuf> {
    Ok(PathBuf::from("/Library/Application Support"))
}

#[inline]
fn get_plugins_dir() -> ResultType<PathBuf> {
    Ok(get_share_dir()?
        .join("RustDesk")
        .join(PLUGIN_SOURCE_LOCAL_DIR))
}

#[inline]
fn get_plugin_dir(id: &str) -> ResultType<PathBuf> {
    Ok(get_plugins_dir()?.join(id))
}

#[inline]
fn get_uninstall_file_path() -> ResultType<PathBuf> {
    Ok(get_plugins_dir()?.join("uninstall_list"))
}

#[inline]
fn cstr_to_string(cstr: *const c_char) -> ResultType<String> {
    if cstr.is_null() {
        bail!("failed to convert string, the pointer is null");
    }
    Ok(String::from_utf8(unsafe {
        CStr::from_ptr(cstr).to_bytes().to_vec()
    })?)
}

#[inline]
fn str_to_cstr_ret(s: &str) -> *const c_char {
    let mut s = s.as_bytes().to_vec();
    s.push(0);
    unsafe {
        let r = libc::malloc(s.len()) as *mut c_char;
        libc::memcpy(
            r as *mut libc::c_void,
            s.as_ptr() as *const libc::c_void,
            s.len(),
        );
        r
    }
}

#[inline]
fn free_c_ptr(p: *mut c_void) {
    if !p.is_null() {
        unsafe {
            libc::free(p);
        }
    }
}
