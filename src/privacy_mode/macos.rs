// macOS 프라이버시 모드 구현
use super::{PrivacyMode, PrivacyModeState};
use hbb_common::{anyhow::anyhow, ResultType};

/// macOS 네이티브 함수: 프라이버시 모드 설정
extern "C" {
    fn MacSetPrivacyMode(on: bool) -> bool;
}

// 프라이버시 모드 구현 식별자
pub const PRIVACY_MODE_IMPL: &str = "privacy_mode_impl_macos";

/// macOS 프라이버시 모드 구현 구조체
/// 화면 공유 중 다른 사용자의 화면 접근 방지
pub struct PrivacyModeImpl {
    /// 구현 식별 키
    impl_key: String,
    /// 현재 프라이버시 모드를 활성화한 연결 ID
    conn_id: i32,
}

impl PrivacyModeImpl {
    /// 새로운 PrivacyModeImpl 생성
    pub fn new(impl_key: &str) -> Self {
        Self {
            impl_key: impl_key.to_owned(),
            conn_id: 0,
        }
    }
}

/// PrivacyMode 트레이트 구현
impl PrivacyMode for PrivacyModeImpl {
    /// 비동기 프라이버시 모드 지원 여부
    fn is_async_privacy_mode(&self) -> bool {
        false
    }

    /// 초기화 (macOS에서는 특별한 초기화 불필요)
    fn init(&self) -> ResultType<()> {
        Ok(())
    }

    /// 프라이버시 모드 정리/해제
    fn clear(&mut self) {
        unsafe {
            MacSetPrivacyMode(false);
        }
        self.conn_id = 0;
    }

    /// 프라이버시 모드 활성화
    fn turn_on_privacy(&mut self, conn_id: i32) -> ResultType<bool> {
        if self.check_on_conn_id(conn_id)? {
            return Ok(true);
        }
        let success = unsafe { MacSetPrivacyMode(true) };
        if !success {
            return Err(anyhow!("Failed to turn on privacy mode"));
        }
        self.conn_id = conn_id;
        Ok(true)
    }

    /// 프라이버시 모드 비활성화
    /// _state 파라미터는 macOS에서 미사용
    /// (Windows에서만 win_topmost_window.rs에서 연결 관리자에 상태 알림용)
    /// macOS는 단순 싱글 모드 구현으로 크로스 컴포넌트 상태 동기화 불필요
    fn turn_off_privacy(&mut self, conn_id: i32, _state: Option<PrivacyModeState>) -> ResultType<()> {
        self.check_off_conn_id(conn_id)?;
        let success = unsafe { MacSetPrivacyMode(false) };
        if !success {
            return Err(anyhow!("Failed to turn off privacy mode"));
        }
        self.conn_id = 0;
        Ok(())
    }

    /// 현재 프라이버시 모드를 활성화한 연결 ID 반환
    fn pre_conn_id(&self) -> i32 {
        self.conn_id
    }

    /// 구현 식별자 반환
    fn get_impl_key(&self) -> &str {
        &self.impl_key
    }
}

/// Drop 구현: 프라이버시 모드 정리
impl Drop for PrivacyModeImpl {
    fn drop(&mut self) {
        // conn_id 일관성 유지 및 모든 정리 작업 중앙화를 위해
        // 다른 코드 경로와 동일한 정리 로직 사용
        self.clear();
    }
}
