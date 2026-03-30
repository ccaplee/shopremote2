// 키보드 입력 처리 모듈
mod keyboard;
/// cbindgen:ignore
// 플랫폼별 기능을 제공하는 모듈 (Windows, macOS, Linux 등)
pub mod platform;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use platform::{
    clip_cursor, get_cursor, get_cursor_data, get_cursor_pos, get_focused_display,
    set_cursor_pos, start_os_service,
};
#[cfg(not(any(target_os = "ios")))]
/// cbindgen:ignore
// 원격 제어를 위한 서버 기능 모듈
mod server;
#[cfg(not(any(target_os = "ios")))]
pub use self::server::*;
// 원격 접속 클라이언트 기능 모듈
mod client;
// 로컬 네트워크(LAN) 발견 모듈
mod lan;
#[cfg(not(any(target_os = "ios")))]
// 랑데뷰 중계자 모듈 - P2P 연결 중재 역할
mod rendezvous_mediator;
#[cfg(not(any(target_os = "ios")))]
pub use self::rendezvous_mediator::*;
/// cbindgen:ignore
// 공통 유틸리티, 설정, 상수 등을 제공하는 모듈
pub mod common;
#[cfg(not(any(target_os = "ios")))]
// 프로세스 간 통신(IPC) 모듈
pub mod ipc;
#[cfg(not(any(
    target_os = "android",
    target_os = "ios",
    feature = "cli",
    feature = "flutter"
)))]
// 사용자 인터페이스 모듈
pub mod ui;
// 버전 정보 모듈
mod version;
pub use version::*;
#[cfg(any(target_os = "android", target_os = "ios", feature = "flutter"))]
// Dart/Flutter 바인딩 생성 코드
mod bridge_generated;
#[cfg(any(target_os = "android", target_os = "ios", feature = "flutter"))]
// Flutter UI 통합 모듈
pub mod flutter;
#[cfg(any(target_os = "android", target_os = "ios", feature = "flutter"))]
// Flutter FFI(Foreign Function Interface) 바인딩
pub mod flutter_ffi;
use common::*;
// 2단계 인증(2FA) 모듈
mod auth_2fa;
#[cfg(feature = "cli")]
// 명령줄 인터페이스(CLI) 모듈
pub mod cli;
#[cfg(not(target_os = "ios"))]
// 클립보드 기능 모듈
mod clipboard;
#[cfg(not(any(target_os = "android", target_os = "ios", feature = "cli")))]
// 핵심 메인 함수 및 초기화 로직
pub mod core_main;
// 커스텀 서버 설정 모듈
mod custom_server;
// 다국어 지원(локализация) 모듈
mod lang;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
// 포트 포워딩 기능 모듈
mod port_forward;

#[cfg(all(feature = "flutter", feature = "plugin_framework"))]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
// 플러그인 프레임워크 모듈
pub mod plugin;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
// 시스템 트레이 아이콘 및 메뉴 모듈
mod tray;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
// 화이트보드 기능 모듈
mod whiteboard;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
// 자동 업데이트 관리 모듈
mod updater;

// 연결 관리자 UI 인터페이스
mod ui_cm_interface;
// 메인 UI 인터페이스
mod ui_interface;
// 세션 UI 인터페이스
mod ui_session_interface;

// HTTP 기반 hbbs(자체 호스팅 서버) 통신 모듈
mod hbbs_http;

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
// 클립보드 파일 관리 모듈
pub mod clipboard_file;

// 개인정보 보호 모드(Privacy Mode) 모듈
pub mod privacy_mode;

#[cfg(windows)]
// 가상 디스플레이 관리 모듈 (IDD - Indirect Display Driver)
pub mod virtual_display_manager;

// KCP 프로토콜 기반 스트림 모듈
mod kcp_stream;
