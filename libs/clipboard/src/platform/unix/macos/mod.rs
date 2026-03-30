/// macOS 클립보드 지원을 위한 Pasteboard API 래퍼
mod item_data_provider;
mod paste_observer;
mod paste_task;
pub mod pasteboard_context;

/// 주어진 클립보드 메시지가 macOS에서 처리 가능한지 확인합니다.
/// Pasteboard 관련 메시지만 처리합니다.
pub fn should_handle_msg(msg: &crate::ClipboardFile) -> bool {
    matches!(
        msg,
        crate::ClipboardFile::FormatList { .. }
            | crate::ClipboardFile::FormatDataResponse { .. }
            | crate::ClipboardFile::FileContentsResponse { .. }
            | crate::ClipboardFile::TryEmpty
    )
}
