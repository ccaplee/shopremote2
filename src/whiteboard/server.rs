use super::CustomEvent;
use crate::ipc::{new_listener, Connection, Data};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use hbb_common::tokio::sync::mpsc::unbounded_channel;
#[cfg(any(target_os = "windows", target_os = "linux"))]
use hbb_common::ResultType;
use hbb_common::{
    allow_err, log,
    tokio::{self, sync::mpsc::UnboundedReceiver},
};
use lazy_static::lazy_static;
use std::sync::RwLock;
use std::time::{Duration, Instant};

#[cfg(any(target_os = "windows", target_os = "macos"))]
use tao::event_loop::EventLoopProxy;
#[cfg(target_os = "linux")]
use winit::event_loop::EventLoopProxy;

lazy_static! {
    /// 이벤트 루프 프록시 - 다른 스레드에서 UI 이벤트를 전송하기 위해 사용
    pub(super) static ref EVENT_PROXY: RwLock<Option<EventLoopProxy<(String, CustomEvent)>>> =
        RwLock::new(None);
}

/// 클릭 리플 애니메이션 지속 시간
const RIPPLE_DURATION: Duration = Duration::from_millis(500);

// 플랫폼별 리플 애니메이션 좌표 타입
#[cfg(target_os = "macos")]
type RippleFloat = f64;
#[cfg(any(target_os = "windows", target_os = "linux"))]
type RippleFloat = f32;

// Linux에서는 독립적인 run 함수 사용
#[cfg(target_os = "linux")]
pub use super::linux::run;

/// 화이트보드 이벤트 루프를 시작합니다.
/// IPC 서버와 UI 렌더링 루프를 동시에 실행합니다.
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn run() {
    // IPC 서버 종료 신호를 위한 채널 생성
    let (tx_exit, rx_exit) = unbounded_channel();
    // IPC 서버를 별도 스레드에서 실행
    std::thread::spawn(move || {
        start_ipc(rx_exit);
    });
    // UI 이벤트 루프 시작
    if let Err(e) = super::create_event_loop() {
        log::error!("이벤트 루프 생성 실패: {}", e);
        tx_exit.send(()).ok();
        return;
    }
}

/// IPC 서버를 시작합니다.
/// 클라이언트 연결을 수신하고 각 연결에 대해 독립적인 작업 스레드를 생성합니다.
#[tokio::main(flavor = "current_thread")]
pub(super) async fn start_ipc(mut rx_exit: UnboundedReceiver<()>) {
    match new_listener("_whiteboard").await {
        Ok(mut incoming) => loop {
            tokio::select! {
                // 종료 신호 수신
                _ = rx_exit.recv() => {
                    log::info!("IPC 서버 종료");
                    break;
                }
                // 클라이언트 연결 수신
                res = incoming.next() => match res {
                    Some(result) => match result {
                        Ok(stream) => {
                            log::debug!("새로운 클라이언트 연결 수신");
                            // 각 클라이언트를 독립적인 작업으로 처리
                            tokio::spawn(handle_new_stream(Connection::new(stream)));
                        }
                        Err(err) => {
                            log::error!("화이트보드 클라이언트 수신 실패: {:?}", err);
                        }
                    },
                    None => {
                        log::error!("화이트보드 클라이언트 수신 실패");
                    }
                }
            }
        },
        Err(err) => {
            log::error!("화이트보드 IPC 서버 시작 실패: {}", err);
        }
    }
}

/// 새로운 클라이언트 연결을 처리합니다.
/// 클라이언트로부터 데이터를 수신하고 UI 이벤트 루프로 전송합니다.
async fn handle_new_stream(mut conn: Connection) {
    loop {
        tokio::select! {
            // 클라이언트로부터 데이터 수신
            res = conn.next() => {
                match res {
                    // 연결 오류 발생
                    Err(err) => {
                        log::info!("화이트보드 IPC 연결 종료: {}", err);
                        break;
                    }
                    // 데이터 수신
                    Ok(Some(data)) => {
                        match data {
                            Data::Whiteboard((k, evt)) => {
                                // 종료 신호 수신
                                if matches!(evt, CustomEvent::Exit) {
                                    log::info!("화이트보드 IPC 연결 종료");
                                    break;
                                } else {
                                    // 이벤트를 UI 이벤트 루프로 전송
                                    EVENT_PROXY.read().unwrap().as_ref().map(|ep| {
                                        allow_err!(ep.send_event((k, evt)));
                                    });
                                }
                            }
                            _ => {}
                        }
                    }
                    // 연결 종료
                    Ok(None) => {
                        log::info!("화이트보드 IPC 연결 종료");
                        break;
                    }
                }
            }
        }
    }
    // 이벤트 루프에 종료 신호 전송
    EVENT_PROXY.read().unwrap().as_ref().map(|ep| {
        allow_err!(ep.send_event(("".to_string(), CustomEvent::Exit)));
    });
}

/// 모든 디스플레이의 통합 사각형 영역을 계산합니다.
/// 다중 모니터 설정에서 전체 가상 스크린 영역을 파악하는 데 사용됩니다.
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub(super) fn get_displays_rect() -> ResultType<(i32, i32, u32, u32)> {
    let displays = crate::server::display_service::try_get_displays()?;
    // 전체 디스플레이 영역을 계산하기 위한 경계값 초기화
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

    // 모든 디스플레이의 경계 계산
    for display in displays {
        let (x, y) = (display.origin().0 as i32, display.origin().1 as i32);
        let (w, h) = (display.width() as i32, display.height() as i32);
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
    }
    // 통합 사각형의 위치와 크기 계산
    let (x, y) = (min_x, min_y);
    let (w, h) = ((max_x - min_x) as u32, (max_y - min_y) as u32);
    Ok((x, y, w, h))
}

/// ARGB 색상값을 RGBA 형식으로 변환합니다.
/// ARGB: Alpha|Red|Green|Blue -> RGBA: Red|Green|Blue|Alpha
#[inline]
pub(super) fn argb_to_rgba(argb: u32) -> (u8, u8, u8, u8) {
    (
        (argb >> 16 & 0xFF) as u8,  // Red
        (argb >> 8 & 0xFF) as u8,   // Green
        (argb & 0xFF) as u8,        // Blue
        (argb >> 24 & 0xFF) as u8,  // Alpha
    )
}

/// 클릭 시 나타나는 리플 애니메이션 효과를 나타내는 구조체
pub(super) struct Ripple {
    /// 리플 중심의 X 좌표
    pub x: RippleFloat,
    /// 리플 중심의 Y 좌표
    pub y: RippleFloat,
    /// 리플 시작 시각
    pub start_time: Instant,
}

impl Ripple {
    /// 아직 활성 상태인 리플만 유지하고 만료된 리플은 제거합니다.
    #[inline]
    pub fn retain_active(ripples: &mut Vec<Ripple>) {
        ripples.retain(|r| r.start_time.elapsed() < RIPPLE_DURATION);
    }

    /// 경과 시간에 따른 리플의 반지름과 투명도를 계산합니다.
    /// 애니메이션 진행도에 따라 점진적으로 확산되고 사라집니다.
    pub fn get_radius_alpha(&self) -> (RippleFloat, RippleFloat) {
        let elapsed = self.start_time.elapsed();
        // 진행도 계산 (0.0 ~ 1.0)
        #[cfg(target_os = "macos")]
        let progress = (elapsed.as_secs_f64() / RIPPLE_DURATION.as_secs_f64()).min(1.0);
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let progress = (elapsed.as_secs_f32() / RIPPLE_DURATION.as_secs_f32()).min(1.0);
        // 플랫폼별 리플 반지름 계산
        #[cfg(target_os = "macos")]
        let radius = 25.0 * progress;
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let radius = 45.0 * progress;
        // 투명도는 진행도가 증가하면서 감소 (0.0 ~ 1.0 -> 1.0 ~ 0.0)
        let alpha = 1.0 - progress;
        (radius, alpha)
    }
}
