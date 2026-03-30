use std::{
    collections::HashMap,
    ffi::{c_char, c_void},
    ptr::addr_of_mut,
    sync::{Arc, RwLock},
};

use flutter_rust_bridge::StreamSink;

use crate::{define_method_prefix, flutter_ffi::EventToUI};

/// 세션이 생성되었음을 알리는 메시지 타입
const MSG_TO_UI_TYPE_SESSION_CREATED: &str = "session_created";

use super::PluginNativeHandler;

/// 세션의 RGBA 데이터를 처리하는 콜백 함수 타입
pub type OnSessionRgbaCallback = unsafe extern "C" fn(
    *const c_char,           // 세션 ID
    *mut c_void,             // 원본 데이터
    *mut usize,              // 너비
    *mut usize,              // 높이
    *mut usize,              // 스트라이드
    *mut scrap::ImageFormat, // 이미지 형식
);

/// 세션 관련 네이티브 핸들러
#[derive(Default)]
pub struct PluginNativeSessionHandler {
    /// 플러그인에서 생성한 세션 목록
    sessions: Arc<RwLock<Vec<crate::flutter::FlutterSession>>>,
    /// 세션별 RGBA 콜백 함수
    cbs: Arc<RwLock<HashMap<String, OnSessionRgbaCallback>>>,
}

lazy_static::lazy_static! {
    /// 세션 핸들러의 전역 인스턴스
    pub static ref SESSION_HANDLER: Arc<PluginNativeSessionHandler> = Arc::new(PluginNativeSessionHandler::default());
}

impl PluginNativeHandler for PluginNativeSessionHandler {
    define_method_prefix!("session_");

    fn on_message(
        &self,
        method: &str,
        data: &serde_json::Map<String, serde_json::Value>,
    ) -> Option<super::NR> {
        match method {
            "create_session" => {
                if let Some(id) = data.get("id") {
                    if let Some(id) = id.as_str() {
                        return Some(super::NR {
                            return_type: 1,
                            data: SESSION_HANDLER.create_session(id.to_string()).as_ptr() as _,
                        });
                    }
                }
            }
            "start_session" => {
                if let Some(id) = data.get("id") {
                    if let Some(id) = id.as_str() {
                        let sessions = SESSION_HANDLER.sessions.read().unwrap();
                        for session in sessions.iter() {
                            if session.id == id {
                                let round =
                                    session.connection_round_state.lock().unwrap().new_round();
                                crate::ui_session_interface::io_loop(session.clone(), round);
                            }
                        }
                    }
                }
            }
            "remove_session_hook" => {
                if let Some(id) = data.get("id") {
                    if let Some(id) = id.as_str() {
                        SESSION_HANDLER.remove_session_hook(id.to_string());
                        return Some(super::NR {
                            return_type: 0,
                            data: std::ptr::null(),
                        });
                    }
                }
            }
            "remove_session" => {
                if let Some(id) = data.get("id") {
                    if let Some(id) = id.as_str() {
                        SESSION_HANDLER.remove_session(id.to_owned());
                        return Some(super::NR {
                            return_type: 0,
                            data: std::ptr::null(),
                        });
                    }
                }
            }
            _ => {}
        }
        None
    }

    fn on_message_raw(
        &self,
        method: &str,
        data: &serde_json::Map<String, serde_json::Value>,
        raw: *const std::ffi::c_void,
        _raw_len: usize,
    ) -> Option<super::NR> {
        match method {
            "add_session_hook" => {
                if let Some(id) = data.get("id") {
                    if let Some(id) = id.as_str() {
                        let cb: OnSessionRgbaCallback = unsafe { std::mem::transmute(raw) };
                        SESSION_HANDLER.add_session_hook(id.to_string(), cb);
                        return Some(super::NR {
                            return_type: 0,
                            data: std::ptr::null(),
                        });
                    }
                }
            }
            _ => {}
        }
        None
    }
}

impl PluginNativeSessionHandler {
    /// 새 세션을 생성하고 Flutter에 알립니다
    fn create_session(&self, session_id: String) -> String {
        let session =
            crate::flutter::session_add(&session_id, false, false, false, "", false, "".to_owned());
        if let Ok(session) = session {
            let mut sessions = self.sessions.write().unwrap();
            sessions.push(session);
            // Flutter이 이 세션의 이벤트 스트림을 바인드하도록 알립니다
            let mut m = HashMap::new();
            m.insert("name", MSG_TO_UI_TYPE_SESSION_CREATED);
            m.insert("session_id", &session_id);
            // TODO: APP_TYPE_DESKTOP_REMOTE은 더 이상 사용되지 않습니다.
            // crate::flutter::APP_TYPE_DESKTOP_REMOTE + 윈도우 ID가 다중 윈도우 지원에 사용됩니다.
            crate::flutter::push_global_event(
                crate::flutter::APP_TYPE_DESKTOP_REMOTE,
                serde_json::to_string(&m).unwrap_or("".to_string()),
            );
            return session_id;
        } else {
            return "".to_string();
        }
    }

    /// 세션에 RGBA 콜백을 추가합니다
    fn add_session_hook(&self, session_id: String, cb: OnSessionRgbaCallback) {
        let sessions = self.sessions.read().unwrap();
        for session in sessions.iter() {
            if session.id == session_id {
                self.cbs.write().unwrap().insert(session_id.to_owned(), cb);
                session.ui_handler.add_session_hook(
                    session_id,
                    crate::flutter::SessionHook::OnSessionRgba(session_rgba_cb),
                );
                break;
            }
        }
    }

    /// 세션에서 RGBA 콜백을 제거합니다
    fn remove_session_hook(&self, session_id: String) {
        let sessions = self.sessions.read().unwrap();
        for session in sessions.iter() {
            if session.id == session_id {
                session.ui_handler.remove_session_hook(&session_id);
            }
        }
    }

    /// 세션을 제거합니다
    fn remove_session(&self, session_id: String) {
        let _ = self.cbs.write().unwrap().remove(&session_id);
        let mut sessions = self.sessions.write().unwrap();
        for i in 0..sessions.len() {
            if sessions[i].id == session_id {
                sessions[i].close_event_stream();
                sessions[i].close();
                sessions.remove(i);
            }
        }
    }

    /// RGBA 데이터를 처리하는 콜백 함수
    #[inline]
    fn session_rgba_cb(&self, session_id: String, rgb: &mut scrap::ImageRgb) {
        let cbs = self.cbs.read().unwrap();
        if let Some(cb) = cbs.get(&session_id) {
            unsafe {
                cb(
                    session_id.as_ptr() as _,
                    rgb.raw.as_mut_ptr() as _,
                    addr_of_mut!(rgb.w),
                    addr_of_mut!(rgb.h),
                    addr_of_mut!(rgb.stride),
                    addr_of_mut!(rgb.fmt),
                );
            }
        }
    }

    /// 세션의 이벤트 스트림을 등록합니다
    #[inline]
    fn session_register_event_stream(&self, session_id: String, stream: StreamSink<EventToUI>) {
        let sessions = self.sessions.read().unwrap();
        for session in sessions.iter() {
            if session.id == session_id {
                *session.event_stream.write().unwrap() = Some(stream);
                break;
            }
        }
    }
}

/// RGBA 콜백을 전달하는 래퍼 함수
#[inline]
fn session_rgba_cb(id: String, rgb: &mut scrap::ImageRgb) {
    SESSION_HANDLER.session_rgba_cb(id, rgb);
}

/// 세션의 이벤트 스트림을 등록하는 공개 함수
#[inline]
pub fn session_register_event_stream(id: String, stream: StreamSink<EventToUI>) {
    SESSION_HANDLER.session_register_event_stream(id, stream);
}
