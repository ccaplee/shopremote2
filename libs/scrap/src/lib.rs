//! Scrap 라이브러리는 화면 캡처 및 인코딩 기능을 제공합니다.
//! 다양한 플랫폼(Windows, macOS, Linux, Android)을 지원하며,
//! 비디오 코덱(VPX, H.264, H.265 등)과 하드웨어 가속을 활용합니다.

#[cfg(quartz)]
extern crate block;
#[macro_use]
extern crate cfg_if;
pub use hbb_common::libc;
#[cfg(dxgi)]
extern crate winapi;

pub use common::*;

/// macOS 화면 캡처 구현 (Quartz 기반)
#[cfg(quartz)]
pub mod quartz;

/// X11 화면 캡처 구현 (Linux)
#[cfg(x11)]
pub mod x11;

/// Wayland 화면 캡처 구현 (Linux Wayland)
#[cfg(all(x11, feature = "wayland"))]
pub mod wayland;

/// DXGI 화면 캡처 구현 (Windows)
#[cfg(dxgi)]
pub mod dxgi;

/// Android 화면 캡처 구현
#[cfg(target_os = "android")]
pub mod android;

/// 공통 코덱 및 변환 기능
mod common;
