use hbb_common::platform::windows::is_windows_version_or_greater;

pub use super::win_topmost_window::PrivacyModeImpl;

// 윈도우 화면 캡처 제외(SetWindowDisplayAffinity)를 사용하는 프라이버시 모드 구현 식별자
pub(super) const PRIVACY_MODE_IMPL: &str = super::PRIVACY_MODE_IMPL_WIN_EXCLUDE_FROM_CAPTURE;

/// SetWindowDisplayAffinity API를 사용한 윈도우 화면 캡처 제외 기능이 지원되는지 확인합니다.
///
/// Windows 10 버전 19041 이상에서만 SetWindowDisplayAffinity API가 지원됩니다.
///
/// 참고:
/// - https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity
/// - https://en.wikipedia.org/wiki/Windows_10_version_history
///
/// # 반환값
/// - true: SetWindowDisplayAffinity API가 지원됨
/// - false: 지원되지 않음 (Windows 10 19041 이전 버전)
pub(super) fn is_supported() -> bool {
    is_windows_version_or_greater(10, 0, 19041, 0, 0)
}
