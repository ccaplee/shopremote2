// macOS Core Graphics (Quartz) 기반 화면 캡처 구현

/// macOS의 화면 캡처 구현
pub use self::capturer::Capturer;
/// 화면 캡처 설정
pub use self::config::Config;
/// Quartz 디스플레이 정보
pub use self::display::Display;
/// Core Graphics FFI 정의 및 에러, 픽셀 형식
pub use self::ffi::{CGError, PixelFormat};
/// 캡처된 프레임 데이터
pub use self::frame::Frame;

mod capturer;
mod config;
mod display;
pub mod ffi;
mod frame;

use std::sync::{Arc, Mutex};

lazy_static::lazy_static! {
    /// Retina 디스플레이(고해상도) 캡처 활성화 여부
    pub static ref ENABLE_RETINA: Arc<Mutex<bool>> = Arc::new(Mutex::new(true));
}
