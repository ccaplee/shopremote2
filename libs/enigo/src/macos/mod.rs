/// macOS 구현
mod macos_impl;

/// macOS 키 코드 정의
pub mod keycodes;
pub use self::macos_impl::{Enigo, ENIGO_INPUT_EXTRA_VALUE};
