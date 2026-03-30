use std::{collections::HashMap, ffi::c_void, os::raw::c_int};

use serde_json::json;

use crate::{define_method_prefix, flutter::APP_TYPE_MAIN};

use super::PluginNativeHandler;

/// UI 인터페이스 관련 네이티브 핸들러
#[derive(Default)]
pub struct PluginNativeUIHandler;

/// UI 콜백 함수의 반환값을 처리하는 함수 타입
///
/// # 주의
/// 네이티브 콜백을 u64로 변환하여 Flutter로 전송하고,
/// Flutter 스레드가 이 콜백을 직접 호출합니다.
///
/// `data` 파라미터의 예시:
/// ```json
/// {
///     "cb": 0x1234567890
/// }
/// ```
///
/// # 안전성
/// 제공한 콜백이 유효한지 확인하세요. 그렇지 않으면 메모리 오류나 호출 문제로 인해 프로그램이 충돌할 수 있습니다!
pub type OnUIReturnCallback =
    extern "C" fn(return_code: c_int, data: *const c_void, data_len: u64, user_data: *const c_void);

impl PluginNativeHandler for PluginNativeUIHandler {
    define_method_prefix!("ui_");

    fn on_message(
        &self,
        method: &str,
        data: &serde_json::Map<String, serde_json::Value>,
    ) -> Option<super::NR> {
        match method {
            "select_peers_async" => {
                if let Some(cb) = data.get("cb") {
                    if let Some(cb) = cb.as_u64() {
                        let user_data = match data.get("user_data") {
                            Some(user_data) => user_data.as_u64().unwrap_or(0),
                            None => 0,
                        };
                        self.select_peers_async(cb, user_data);
                        return Some(super::NR {
                            return_type: 0,
                            data: std::ptr::null(),
                        });
                    }
                }
                return Some(super::NR {
                    return_type: -1,
                    data: "missing cb field message".as_ptr() as _,
                });
            }
            "register_ui_entry" => {
                let title;
                if let Some(v) = data.get("title") {
                    title = v.as_str().unwrap_or("");
                } else {
                    title = "";
                }
                if let Some(on_tap_cb) = data.get("on_tap_cb") {
                    if let Some(on_tap_cb) = on_tap_cb.as_u64() {
                        let user_data = match data.get("user_data") {
                            Some(user_data) => user_data.as_u64().unwrap_or(0),
                            None => 0,
                        };
                        self.register_ui_entry(title, on_tap_cb, user_data);
                        return Some(super::NR {
                            return_type: 0,
                            data: std::ptr::null(),
                        });
                    }
                }
                return Some(super::NR {
                    return_type: -1,
                    data: "missing cb field message".as_ptr() as _,
                });
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
        None
    }
}

impl PluginNativeUIHandler {
    /// 피어 선택 다이얼로그를 비동기로 열고 결과를 콜백으로 받습니다
    ///
    /// # 호출 방식
    /// `select_peers_async` 메서드를 다음 JSON으로 호출합니다:
    /// ```json
    /// {
    ///     "cb": 0,          // 함수 주소 (OnUIReturnCallback 타입)
    ///     "user_data": 0    // 콜백으로 전달될 불투명한 포인터 값
    /// }
    /// ```
    fn select_peers_async(&self, cb: u64, user_data: u64) {
        let mut param = HashMap::new();
        param.insert("name", json!("native_ui"));
        param.insert("action", json!("select_peers"));
        param.insert("cb", json!(cb));
        param.insert("user_data", json!(user_data));
        crate::flutter::push_global_event(
            APP_TYPE_MAIN,
            serde_json::to_string(&param).unwrap_or("".to_string()),
        );
    }

    /// UI에 새 항목을 등록합니다
    ///
    /// # 호출 방식
    /// `register_ui_entry` 메서드를 다음 JSON으로 호출합니다:
    /// ```json
    /// {
    ///     "on_tap_cb": 0,     // 함수 주소 (클릭 시 호출)
    ///     "user_data": 0,     // 콜백으로 전달될 불투명한 포인터 값
    ///     "title": "entry name" // 항목의 이름
    /// }
    /// ```
    fn register_ui_entry(&self, title: &str, on_tap_cb: u64, user_data: u64) {
        let mut param = HashMap::new();
        param.insert("name", json!("native_ui"));
        param.insert("action", json!("register_ui_entry"));
        param.insert("title", json!(title));
        param.insert("cb", json!(on_tap_cb));
        param.insert("user_data", json!(user_data));
        crate::flutter::push_global_event(
            APP_TYPE_MAIN,
            serde_json::to_string(&param).unwrap_or("".to_string()),
        );
    }
}
