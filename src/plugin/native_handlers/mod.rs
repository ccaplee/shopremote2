use std::{
    ffi::c_void,
    sync::{Arc, RwLock},
    vec,
};

use hbb_common::libc::c_char;
use lazy_static::lazy_static;
use serde_json::Map;

use crate::return_if_not_method;

use self::{session::PluginNativeSessionHandler, ui::PluginNativeUIHandler};

use super::cstr_to_string;

mod macros;
pub mod session;
pub mod ui;

/// NativeReturnValue의 단축 이름
pub type NR = super::native::NativeReturnValue;
/// 네이티브 핸들러 레지스트러의 타입 별칭
pub type PluginNativeHandlerRegistrar = NativeHandlerRegistrar<Box<dyn Callable + Send + Sync>>;

lazy_static! {
    /// 네이티브 핸들러를 등록하고 관리하는 전역 레지스트러
    pub static ref NATIVE_HANDLERS_REGISTRAR: Arc<PluginNativeHandlerRegistrar> =
        Arc::new(PluginNativeHandlerRegistrar::default());
}

/// 네이티브 핸들러를 관리하는 레지스트러
#[derive(Clone)]
pub struct NativeHandlerRegistrar<H> {
    /// 등록된 핸들러들의 목록
    handlers: Arc<RwLock<Vec<H>>>,
}

impl Default for PluginNativeHandlerRegistrar {
    fn default() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(vec![
                // 미리 빌드된 네이티브 핸들러를 여기에 추가합니다.
                Box::new(PluginNativeSessionHandler::default()),
                Box::new(PluginNativeUIHandler::default()),
            ])),
        }
    }
}

/// 네이티브 핸들러의 기본 트레이트
pub(self) trait PluginNativeHandler {
    /// 이 핸들러가 처리할 메서드의 접두사
    fn method_prefix(&self) -> &'static str;

    /// 주어진 데이터로 메서드를 처리합니다.
    ///
    /// # 반환값
    /// 이 핸들러가 처리하지 못한 메시지의 경우 None 반환
    fn on_message(&self, method: &str, data: &Map<String, serde_json::Value>) -> Option<NR>;

    /// 주어진 데이터와 추가 바이너리 데이터로 메서드를 처리합니다.
    ///
    /// # 반환값
    /// 이 핸들러가 처리하지 못한 메시지의 경우 None 반환
    fn on_message_raw(
        &self,
        method: &str,
        data: &Map<String, serde_json::Value>,
        raw: *const c_void,
        raw_len: usize,
    ) -> Option<NR>;
}

/// 호출 가능한 핸들러 트레이트
pub trait Callable {
    /// C 인터페이스로부터의 호출을 처리합니다
    fn call(
        &self,
        method: &String,
        json: *const c_char,
        raw: *const c_void,
        raw_len: usize,
    ) -> Option<NR> {
        None
    }
}

/// PluginNativeHandler를 구현하는 모든 타입이 Callable을 구현하도록 합니다
impl<T> Callable for T
where
    T: PluginNativeHandler + Send + Sync,
{
    fn call(
        &self,
        method: &String,
        json: *const c_char,
        raw: *const c_void,
        raw_len: usize,
    ) -> Option<NR> {
        let prefix = self.method_prefix();
        // 메서드가 해당 접두사로 시작하지 않으면 None 반환
        return_if_not_method!(method, prefix);
        match cstr_to_string(json) {
            Ok(s) => {
                // JSON 파싱
                if let Ok(json) = serde_json::from_str(s.as_str()) {
                    let method_suffix = &method[prefix.len()..];
                    // 바이너리 데이터가 있으면 on_message_raw 호출, 없으면 on_message 호출
                    if raw != std::ptr::null() && raw_len > 0 {
                        return self.on_message_raw(method_suffix, &json, raw, raw_len);
                    } else {
                        return self.on_message(method_suffix, &json);
                    }
                } else {
                    return None;
                }
            }
            Err(_) => return None,
        }
    }
}

/// 핸들러 레지스트러가 Callable을 구현하도록 합니다
impl Callable for PluginNativeHandlerRegistrar {
    fn call(
        &self,
        method: &String,
        json: *const c_char,
        raw: *const c_void,
        raw_len: usize,
    ) -> Option<NR> {
        // 등록된 모든 핸들러에 메서드 호출을 시도합니다
        for handler in self.handlers.read().unwrap().iter() {
            let ret = handler.call(method, json, raw, raw_len);
            // 처리된 결과가 있으면 반환
            if ret.is_some() {
                return ret;
            }
        }
        None
    }
}
