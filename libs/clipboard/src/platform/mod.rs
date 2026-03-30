/// Windows 플랫폼 클립보드 구현
#[cfg(target_os = "windows")]
pub mod windows;

/// Windows 플랫폼에서 클립보드 컨텍스트를 생성합니다.
/// # 인자
/// - `enable_files`: 파일 전송 기능 활성화 여부
/// - `enable_others`: 기타 기능 활성화 여부
/// - `response_wait_timeout_secs`: 응답 대기 시간 (초)
#[cfg(target_os = "windows")]
pub fn create_cliprdr_context(
    enable_files: bool,
    enable_others: bool,
    response_wait_timeout_secs: u32,
) -> crate::ResultType<Box<dyn crate::CliprdrServiceContext>> {
    let boxed =
        windows::create_cliprdr_context(enable_files, enable_others, response_wait_timeout_secs)?
            as Box<_>;
    Ok(boxed)
}

/// Unix 플랫폼(macOS 및 Linux) 클립보드 구현
#[cfg(feature = "unix-file-copy-paste")]
pub mod unix;

/// macOS 플랫폼에서 클립보드 컨텍스트를 생성합니다.
/// macOS에서는 파라미터를 사용하지 않고 pasteboard 컨텍스트를 직접 생성합니다.
#[cfg(all(feature = "unix-file-copy-paste", target_os = "macos"))]
pub fn create_cliprdr_context(
    _enable_files: bool,
    _enable_others: bool,
    _response_wait_timeout_secs: u32,
) -> crate::ResultType<Box<dyn crate::CliprdrServiceContext>> {
    let boxed = unix::macos::pasteboard_context::create_pasteboard_context()? as Box<_>;
    Ok(boxed)
}
