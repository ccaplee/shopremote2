/// Wayland 디스플레이 캡처 가능한 리소스를 정의합니다.
pub mod capturable;
/// PipeWire 기반 화면 캡처 구현
pub mod pipewire;
/// Wayland 디스플레이 정보 조회
pub mod display;
/// XDG Desktop Portal ScreenCast 인터페이스
mod screencast_portal;
/// XDG Desktop Portal Request 인터페이스
mod request_portal;
/// XDG Desktop Portal RemoteDesktop 인터페이스
pub mod remote_desktop_portal;
