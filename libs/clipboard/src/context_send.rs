use hbb_common::{log, ResultType};
use std::{ops::Deref, sync::Mutex};

use crate::CliprdrServiceContext;

/// 클립보드 응답 대기 시간 (초)
const CLIPBOARD_RESPONSE_WAIT_TIMEOUT_SECS: u32 = 30;

/// 전역 클립보드 전송 컨텍스트 인스턴스
lazy_static::lazy_static! {
    static ref CONTEXT_SEND: ContextSend = ContextSend::default();
}

/// 클립보드 전송 컨텍스트를 관리하는 래퍼 구조체입니다.
/// 뮤텍스로 보호된 선택적 CliprdrServiceContext를 래핑합니다.
#[derive(Default)]
pub struct ContextSend(Mutex<Option<Box<dyn CliprdrServiceContext>>>);

/// Deref 트레이트 구현으로 ContextSend를 내부 Mutex처럼 사용할 수 있게 합니다.
impl Deref for ContextSend {
    type Target = Mutex<Option<Box<dyn CliprdrServiceContext>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ContextSend {
    /// 클립보드 컨텍스트가 활성화되어 있는지 확인합니다.
    #[inline]
    pub fn is_enabled() -> bool {
        CONTEXT_SEND.lock().unwrap().is_some()
    }

    /// 클립보드 컨텍스트를 중지 상태로 설정합니다.
    pub fn set_is_stopped() {
        let _res = Self::proc(|c| c.set_is_stopped().map_err(|e| e.into()));
    }

    /// 클립보드 컨텍스트를 활성화하거나 비활성화합니다.
    /// 활성화 시 새로운 cliprdr 컨텍스트를 생성하고, 비활성화 시 기존 컨텍스트를 제거합니다.
    pub fn enable(enabled: bool) {
        let mut lock = CONTEXT_SEND.lock().unwrap();
        if enabled {
            if lock.is_some() {
                return;
            }
            match crate::create_cliprdr_context(true, false, CLIPBOARD_RESPONSE_WAIT_TIMEOUT_SECS) {
                Ok(context) => {
                    log::info!("clipboard context for file transfer created.");
                    *lock = Some(context)
                }
                Err(err) => {
                    log::error!(
                        "create clipboard context for file transfer: {}",
                        err.to_string()
                    );
                }
            }
        } else if let Some(_clp) = lock.take() {
            *lock = None;
            log::info!("clipboard context for file transfer destroyed.");
        }
    }

    /// 클립보드 컨텍스트가 활성화되어 있는지 확인하고, 필요하면 생성합니다.
    /// 컨텍스트가 없으면 새로 생성하고, 이미 있으면 그대로 유지합니다.
    pub fn make_sure_enabled() -> ResultType<()> {
        let mut lock = CONTEXT_SEND.lock().unwrap();
        if lock.is_some() {
            return Ok(());
        }

        let ctx = crate::create_cliprdr_context(true, false, CLIPBOARD_RESPONSE_WAIT_TIMEOUT_SECS)?;
        *lock = Some(ctx);
        log::info!("clipboard context for file transfer recreated.");
        Ok(())
    }

    /// 클립보드 컨텍스트에서 클로저를 실행합니다.
    /// 컨텍스트가 있으면 실행하고, 없으면 무시합니다.
    pub fn proc<F: FnOnce(&mut Box<dyn CliprdrServiceContext>) -> ResultType<()>>(
        f: F,
    ) -> ResultType<()> {
        let mut lock = CONTEXT_SEND.lock().unwrap();
        match lock.as_mut() {
            Some(context) => f(context),
            None => Ok(()),
        }
    }
}
