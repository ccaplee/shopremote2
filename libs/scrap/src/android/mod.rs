// Android 플랫폼 지원을 위한 FFI 정의

/// Android FFI 인터페이스
pub mod ffi;

// FFI에서 정의한 모든 항목 재내보내기
pub use ffi::*;
