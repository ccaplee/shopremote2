use hbb_common::log;
use std::ffi::c_char;

/// Trace 로그 레벨 상수
const LOG_LEVEL_TRACE: &[u8; 6] = b"trace\0";
/// Debug 로그 레벨 상수
const LOG_LEVEL_DEBUG: &[u8; 6] = b"debug\0";
/// Info 로그 레벨 상수
const LOG_LEVEL_INFO: &[u8; 5] = b"info\0";
/// Warn 로그 레벨 상수
const LOG_LEVEL_WARN: &[u8; 5] = b"warn\0";
/// Error 로그 레벨 상수
const LOG_LEVEL_ERROR: &[u8; 6] = b"error\0";

/// 주어진 로그 레벨이 특정 레벨과 일치하는지 확인합니다
#[inline]
fn is_level(level: *const c_char, level_bytes: &[u8]) -> bool {
    level_bytes == unsafe { std::slice::from_raw_parts(level as *const u8, level_bytes.len()) }
}

/// 플러그인으로부터 로그 메시지를 받는 콜백 함수
///
/// # 매개변수
/// * `level` - 로그 레벨 ("trace", "debug", "info", "warn", "error")
/// * `msg` - 로그 메시지 (UTF-8 널 종료 문자열)
#[no_mangle]
pub(super) extern "C" fn plugin_log(level: *const c_char, msg: *const c_char) {
    if level.is_null() || msg.is_null() {
        return;
    }

    if let Ok(msg) = super::cstr_to_string(msg) {
        // 로그 레벨에 따라 적절한 로그 함수 호출
        if is_level(level, LOG_LEVEL_TRACE) {
            log::trace!("{}", msg);
        } else if is_level(level, LOG_LEVEL_DEBUG) {
            log::debug!("{}", msg);
        } else if is_level(level, LOG_LEVEL_INFO) {
            log::info!("{}", msg);
        } else if is_level(level, LOG_LEVEL_WARN) {
            log::warn!("{}", msg);
        } else if is_level(level, LOG_LEVEL_ERROR) {
            log::error!("{}", msg);
        }
    }
}
