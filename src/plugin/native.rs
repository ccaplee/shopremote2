use std::{
    ffi::{c_char, c_int, c_void},
    os::raw::c_uint,
};

use hbb_common::log::error;

use super::{
    cstr_to_string,
    errno::ERR_NOT_HANDLED,
    native_handlers::{Callable, NATIVE_HANDLERS_REGISTRAR},
};

/// librustdesk 네이티브에서 반환된 값을 나타내는 구조체
///
/// [주의]
/// 데이터는 librustdesk가 소유합니다.
#[repr(C)]
pub struct NativeReturnValue {
    /// 반환 타입 또는 오류 코드
    pub return_type: c_int,
    /// 반환 데이터 포인터
    pub data: *const c_void,
}

// 플러그인으로부터 받은 네이티브 콜을 처리하는 콜백 함수
//
// # 매개변수
// * `method` - 호출할 메서드 이름 (UTF-8 널 종료 문자열)
// * `json` - 메서드 파라미터 (JSON 형식, UTF-8 널 종료 문자열)
// * `raw` - 추가 바이너리 데이터 (선택사항)
// * `raw_len` - 추가 바이너리 데이터의 길이
//
// # 반환값
// 메서드 호출 결과를 포함한 NativeReturnValue
pub(super) extern "C" fn cb_native_data(
    method: *const c_char,
    json: *const c_char,
    raw: *const c_void,
    raw_len: usize,
) -> NativeReturnValue {
    let ret = match cstr_to_string(method) {
        Ok(method) => NATIVE_HANDLERS_REGISTRAR.call(&method, json, raw, raw_len),
        Err(err) => {
            error!("cb_native_data error: {}", err);
            None
        }
    };
    return ret.unwrap_or(NativeReturnValue {
        return_type: ERR_NOT_HANDLED,
        data: std::ptr::null(),
    });
}
