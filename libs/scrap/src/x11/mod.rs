// X11 화면 캡처 구현 모듈
// XCB (X11 C Binding)과 MIT-SHM (Shared Memory)을 사용합니다.

/// X11 디스플레이에서 프레임을 캡처하는 구조체
pub use self::capturer::*;
/// X11 디스플레이 정보 조회
pub use self::display::*;
/// 디스플레이 창 목록 반복기
pub use self::iter::*;
/// X11 서버 연결 관리
pub use self::server::*;

mod capturer;
mod display;
/// X11/XCB FFI 정의
mod ffi;
mod iter;
mod server;
