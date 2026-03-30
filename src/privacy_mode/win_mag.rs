use super::win_topmost_window::PRIVACY_WINDOW_NAME;
use hbb_common::{bail, log, ResultType};
use std::time::Instant;

pub use super::win_topmost_window::PrivacyModeImpl;

// Windows Magnifier API를 사용하는 프라이버시 모드 구현 식별자
pub(super) const PRIVACY_MODE_IMPL: &str = super::PRIVACY_MODE_IMPL_WIN_MAG;

/// Windows Magnifier(돋보기) API를 사용하여 화면 캡처를 생성합니다.
///
/// 이 함수는 Magnifier API를 이용하여 화면을 캡처하고,
/// 프라이버시 보호 윈도우를 제외하도록 설정합니다.
/// 프라이버시 모드가 Magnifier 구현으로 활성화되어 있을 때만 작동합니다.
///
/// # 인자
/// - `privacy_mode_id`: 프라이버시 모드 연결 ID
/// - `origin`: 캡처 영역의 시작 위치 (x, y)
/// - `width`: 캡처 너비 (픽셀)
/// - `height`: 캡처 높이 (픽셀)
///
/// # 반환값
/// - Ok(Some(capturer)): Magnifier 캡처 생성 성공
/// - Ok(None): Magnifier 구현이 현재 활성화되지 않음
/// - Err: 캡처 생성 또는 프라이버시 윈도우 제외 실패
pub fn create_capturer(
    privacy_mode_id: i32,
    origin: (i32, i32),
    width: usize,
    height: usize,
) -> ResultType<Option<scrap::CapturerMag>> {
    // 현재 프라이버시 모드 구현이 Magnifier인지 확인
    if !super::is_current_privacy_mode_impl(PRIVACY_MODE_IMPL) {
        return Ok(None);
    }

    match scrap::CapturerMag::new(origin, width, height) {
        Ok(mut c1) => {
            let mut ok = false;
            let check_begin = Instant::now();
            // 최대 5초 동안 프라이버시 윈도우 제외 설정 재시도
            while check_begin.elapsed().as_secs() < 5 {
                match c1.exclude("", PRIVACY_WINDOW_NAME) {
                    Ok(false) => {
                        // 제외 설정이 아직 적용되지 않음, 재시도 대기
                        ok = false;
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                    Err(e) => {
                        bail!(
                            "Failed to exclude privacy window {} - {}, err: {}",
                            "",
                            PRIVACY_WINDOW_NAME,
                            e
                        );
                    }
                    _ => {
                        // 성공: 프라이버시 윈도우가 제외됨
                        ok = true;
                        break;
                    }
                }
            }
            if !ok {
                bail!(
                    "Failed to exclude privacy window {} - {} ",
                    "",
                    PRIVACY_WINDOW_NAME
                );
            }
            log::debug!("Create magnifier capture for {}", privacy_mode_id);
            Ok(Some(c1))
        }
        Err(e) => {
            bail!(format!("Failed to create magnifier capture {}", e));
        }
    }
}
