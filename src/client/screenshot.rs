#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::clipboard::{update_clipboard, ClipboardSide};
use hbb_common::{message_proto::*, ResultType};
use std::sync::Mutex;

/// 전역 스크린샷 데이터를 저장하는 정적 뮤텍스
/// 스크린샷 캡처 후 클라이언트 측에서 임시 보관하는 용도로 사용
lazy_static::lazy_static! {
    static ref SCREENSHOT: Mutex<Screenshot> = Default::default();
}

/// 스크린샷 처리 후 수행할 액션을 나타내는 열거형
/// - SaveAs(경로): 지정된 파일 경로에 PNG로 저장
/// - CopyToClipboard: 클립보드에 이미지 복사
/// - Discard: 스크린샷 폐기
pub enum ScreenshotAction {
    /// 파일로 저장: 경로 문자열 포함
    SaveAs(String),
    /// 클립보드로 복사
    CopyToClipboard,
    /// 폐기
    Discard,
}

impl Default for ScreenshotAction {
    fn default() -> Self {
        Self::Discard
    }
}

/// 문자열 형식의 액션 코드를 ScreenshotAction으로 변환
/// 형식: "0:경로" (저장), "1" (클립보드), "2" (폐기), 기타 (기본값)
impl From<&str> for ScreenshotAction {
    fn from(value: &str) -> Self {
        match value.chars().next() {
            Some('0') => {
                // 저장 액션: "0:경로" 형식에서 경로 추출
                if let Some((pos, _)) = value.char_indices().nth(2) {
                    let substring = &value[pos..];
                    Self::SaveAs(substring.to_string())
                } else {
                    Self::default()
                }
            }
            Some('1') => Self::CopyToClipboard,
            Some('2') => Self::default(),
            _ => Self::default(),
        }
    }
}

/// ScreenshotAction을 프로토콜 전송 가능한 문자열 형식으로 변환
impl Into<String> for ScreenshotAction {
    fn into(self) -> String {
        match self {
            Self::SaveAs(p) => format!("0:{p}"),
            Self::CopyToClipboard => "1".to_owned(),
            Self::Discard => "2".to_owned(),
        }
    }
}

/// 스크린샷 바이너리 데이터를 임시 저장하는 구조체
/// 서버에서 수신한 스크린샷을 캐싱하고 클라이언트 UI에서 처리할 때까지 보관
#[derive(Default)]
pub struct Screenshot {
    /// PNG 형식의 스크린샷 바이너리 데이터
    data: Option<bytes::Bytes>,
}

impl Screenshot {
    /// 스크린샷 바이너리 데이터 저장
    /// 새 데이터가 들어오면 이전 데이터는 자동으로 덮어씌워짐
    /// data: PNG 형식의 이미지 바이너리
    fn set_screenshot(&mut self, data: bytes::Bytes) {
        self.data.replace(data);
    }

    /// 저장된 스크린샷에 대해 지정된 액션 수행
    /// action: 수행할 액션 (저장/클립보드/폐기)
    /// 반환값: 오류 메시지 (성공시 빈 문자열)
    fn handle_screenshot(&mut self, action: String) -> String {
        let Some(data) = self.data.take() else {
            return "캐시된 스크린샷 없음".to_owned();
        };
        match Self::handle_screenshot_(data, action) {
            Ok(()) => "".to_owned(),
            Err(e) => e.to_string(),
        }
    }

    /// 스크린샷 바이너리 데이터에 대해 실제 액션 수행
    /// data: PNG 형식의 스크린샷 바이너리
    /// action: 수행할 액션 코드
    /// - SaveAs: 지정된 경로에 파일로 저장
    /// - CopyToClipboard: PNG 형식으로 클립보드에 복사 (안드로이드/iOS 제외)
    /// - Discard: 아무것도 하지 않음
    fn handle_screenshot_(data: bytes::Bytes, action: String) -> ResultType<()> {
        match ScreenshotAction::from(&action as &str) {
            ScreenshotAction::SaveAs(p) => {
                // 지정된 경로에 PNG 파일로 저장
                std::fs::write(p, data)?;
            }
            ScreenshotAction::CopyToClipboard => {
                // 안드로이드/iOS 제외 플랫폼에서 클립보드로 복사
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                {
                    let clips = vec![Clipboard {
                        compress: false,
                        content: data,
                        format: ClipboardFormat::ImagePng.into(),
                        ..Default::default()
                    }];
                    update_clipboard(clips, ClipboardSide::Client);
                }
            }
            ScreenshotAction::Discard => {}
        }
        Ok(())
    }
}

/// 전역 스크린샷 저장소에 스크린샷 데이터 저장
/// 서버에서 수신한 스크린샷 바이너리를 임시 저장
/// data: PNG 형식의 스크린샷 바이너리
pub fn set_screenshot(data: bytes::Bytes) {
    SCREENSHOT.lock().unwrap().set_screenshot(data);
}

/// 저장된 스크린샷에 대해 지정된 액션 처리
/// action: 수행할 액션 (저장/클립보드/폐기)
/// 반환값: 처리 결과 메시지 (성공시 빈 문자열, 오류시 오류 메시지)
pub fn handle_screenshot(action: String) -> String {
    SCREENSHOT.lock().unwrap().handle_screenshot(action)
}
