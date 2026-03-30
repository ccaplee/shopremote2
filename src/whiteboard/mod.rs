use serde_derive::{Deserialize, Serialize};

// 화이트보드 클라이언트 모듈
mod client;
// 화이트보드 서버 모듈
mod server;

// 플랫폼별 화이트보드 구현
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod win_linux;

// 플랫폼별 이벤트 루프 임포트
#[cfg(target_os = "windows")]
use windows::create_event_loop;
#[cfg(target_os = "macos")]
use macos::create_event_loop;
#[cfg(target_os = "linux")]
pub use linux::is_supported;

// 공개 API
pub use client::*;
pub use server::*;

/// 화이트보드에서 발생하는 사용자 정의 이벤트들
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "t", content = "c")]
pub enum CustomEvent {
    /// 커서 이동 또는 클릭 이벤트
    Cursor(Cursor),
    /// 화면 초기화 이벤트
    Clear,
    /// 화이트보드 종료 신호
    Exit,
}

/// 커서 정보를 나타내는 구조체
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "t")]
pub struct Cursor {
    // 커서 X 좌표
    pub x: f32,
    // 커서 Y 좌표
    pub y: f32,
    // 커서 색상 (ARGB 형식)
    pub argb: u32,
    // 버튼 상태 (0: 눌리지 않음, 1: 좌클릭, 2: 우클릭, 4: 중간클릭 등)
    pub btns: i32,
    // 커서에 표시할 텍스트 (사용자 이름 등)
    pub text: String,
}
