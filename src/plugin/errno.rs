#![allow(dead_code)]

/// 성공을 나타내는 오류 코드
pub const ERR_SUCCESS: i32 = 0;

// ======================================================
// 플러그인에서 발생하며 RustDesk이 처리해야 하는 오류들

/// RustDesk이 처리해야 하는 오류의 기본 범위 시작
pub const ERR_RUSTDESK_HANDLE_BASE: i32 = 10000;

/// 플러그인이 로드되지 않음
pub const ERR_PLUGIN_LOAD: i32 = 10001;
/// 플러그인이 초기화되지 않음
pub const ERR_PLUGIN_MSG_INIT: i32 = 10101;
/// 플러그인 초기화 데이터가 유효하지 않음
pub const ERR_PLUGIN_MSG_INIT_INVALID: i32 = 10102;
/// 로컬 피어 ID 가져오기 실패
pub const ERR_PLUGIN_MSG_GET_LOCAL_PEER_ID: i32 = 10103;
/// 플러그인 서명이 검증되지 않음
pub const ERR_PLUGIN_SIGNATURE_NOT_VERIFIED: i32 = 10104;
/// 플러그인 서명 검증 실패
pub const ERR_PLUGIN_SIGNATURE_VERIFICATION_FAILED: i32 = 10105;
/// 구현되지 않은 호출
pub const ERR_CALL_UNIMPLEMENTED: i32 = 10201;
/// 유효하지 않은 메서드
pub const ERR_CALL_INVALID_METHOD: i32 = 10202;
/// 지원되지 않는 메서드
pub const ERR_CALL_NOT_SUPPORTED_METHOD: i32 = 10203;
/// 유효하지 않은 피어 ID
pub const ERR_CALL_INVALID_PEER: i32 = 10204;
/// 호출 시 실패 - 유효하지 않은 인자
pub const ERR_CALL_INVALID_ARGS: i32 = 10301;
/// 피어 ID가 일치하지 않음
pub const ERR_PEER_ID_MISMATCH: i32 = 10302;
/// 호출 시 설정 값 오류
pub const ERR_CALL_CONFIG_VALUE: i32 = 10303;
/// 호출 시 핸들러가 없음
pub const ERR_NOT_HANDLED: i32 = 10401;

// ======================================================
// RustDesk 콜백에서 발생하는 오류들

/// RustDesk 콜백 오류의 기본 범위 시작
pub const ERR_CALLBACK_HANDLE_BASE: i32 = 20000;
/// 콜백 오류 - 유효하지 않은 플러그인 ID
pub const ERR_CALLBACK_PLUGIN_ID: i32 = 20001;
/// 콜백 오류 - 유효하지 않은 인자
pub const ERR_CALLBACK_INVALID_ARGS: i32 = 20002;
/// 콜백 오류 - 유효하지 않은 메시지
pub const ERR_CALLBACK_INVALID_MSG: i32 = 20003;
/// 콜백 오류 - 유효하지 않은 대상
pub const ERR_CALLBACK_TARGET: i32 = 20004;
/// 콜백 오류 - 유효하지 않은 대상 타입
pub const ERR_CALLBACK_TARGET_TYPE: i32 = 20005;
/// 콜백 오류 - 피어를 찾을 수 없음
pub const ERR_CALLBACK_PEER_NOT_FOUND: i32 = 20006;

/// 콜백 오류 - 작업 실패
pub const ERR_CALLBACK_FAILED: i32 = 21001;

// ======================================================
// 플러그인에서 발생하며 플러그인이 처리해야 하는 오류들

/// 플러그인이 처리해야 하는 오류의 기본 범위 시작
pub const ERR_PLUGIN_HANDLE_BASE: i32 = 30000;

/// 호출 실패
pub const EER_CALL_FAILED: i32 = 30021;
/// 피어 켜기 실패
pub const ERR_PEER_ON_FAILED: i32 = 40012;
/// 피어 끄기 실패
pub const ERR_PEER_OFF_FAILED: i32 = 40012;
