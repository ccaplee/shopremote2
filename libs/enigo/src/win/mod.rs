/// Windows 구현
mod win_impl;

/// Windows 키 코드 정의
pub mod keycodes;
pub use self::win_impl::{Enigo, ENIGO_INPUT_EXTRA_VALUE};
