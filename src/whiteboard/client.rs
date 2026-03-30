use super::{Cursor, CustomEvent};
use crate::{
    ipc::{self, Data},
    CHILD_PROCESS,
};
use hbb_common::{
    allow_err,
    anyhow::anyhow,
    bail, log, sleep,
    tokio::{
        self,
        sync::mpsc::{unbounded_channel, UnboundedSender},
        time::interval_at,
    },
    ResultType,
};
use lazy_static::lazy_static;
use std::{collections::HashMap, sync::RwLock, time::Instant};

lazy_static! {
    // 화이트보드 이벤트를 전송하기 위한 unbounded 채널 송신자
    static ref TX_WHITEBOARD: RwLock<Option<UnboundedSender<(String, CustomEvent)>>> =
        RwLock::new(None);
    // 화이트보드 연결 정보를 저장하는 해시맵 (키: 연결 ID, 값: 연결 상태)
    static ref CONNS: RwLock<HashMap<String, Conn>> = Default::default();
}

// 화이트보드 연결 상태를 나타내는 구조체
struct Conn {
    // 마지막 커서 위치 (클릭 리플 효과 생성용)
    last_cursor_pos: (f32, f32),
    // 마지막 커서 이벤트 정보
    last_cursor_evt: LastCursorEvent,
}

// 마지막 커서 이벤트를 저장하는 구조체 (이벤트 쓰로틀링용)
struct LastCursorEvent {
    // 저장된 커서 이벤트
    evt: Option<CustomEvent>,
    // 이벤트 발생 시각
    tm: Instant,
    // 이벤트 발생 횟수 카운터
    c: usize,
}

/// 연결 ID를 기반으로 커서 키를 생성합니다.
#[inline]
pub fn get_key_cursor(conn_id: i32) -> String {
    format!("{}-cursor", conn_id)
}

/// 새로운 화이트보드 연결을 등록합니다.
/// 화이트보드 프로세스가 아직 시작되지 않았다면 별도 스레드에서 시작합니다.
pub fn register_whiteboard(k: String) {
    // 화이트보드 서버를 별도 스레드에서 시작
    std::thread::spawn(|| {
        allow_err!(start_whiteboard_());
    });
    // 연결 정보를 저장소에 추가 (중복 체크)
    let mut conns = CONNS.write().unwrap();
    if !conns.contains_key(&k) {
        conns.insert(
            k,
            Conn {
                last_cursor_pos: (0.0, 0.0),
                last_cursor_evt: LastCursorEvent {
                    evt: None,
                    tm: Instant::now(),
                    c: 0,
                },
            },
        );
    }
}

/// 화이트보드 연결을 등록 해제하고 정리합니다.
/// 모든 연결이 종료되면 화이트보드 프로세스를 종료합니다.
pub fn unregister_whiteboard(k: String) {
    // 연결 정보 제거
    let mut conns = CONNS.write().unwrap();
    conns.remove(&k);
    let is_conns_empty = conns.is_empty();
    drop(conns);

    // 화이트보드 서버에 화면 초기화 이벤트 전송
    TX_WHITEBOARD.read().unwrap().as_ref().map(|tx| {
        allow_err!(tx.send((k, CustomEvent::Clear)));
    });

    // 모든 연결이 종료되었다면 화이트보드 프로세스 종료
    if is_conns_empty {
        std::thread::spawn(|| {
            let mut whiteboard = TX_WHITEBOARD.write().unwrap();
            whiteboard.as_ref().map(|tx| {
                allow_err!(tx.send(("".to_string(), CustomEvent::Exit)));
                // 화이트보드 프로세스 종료 대기
                std::thread::sleep(std::time::Duration::from_millis(3_00));
            });
            whiteboard.take();
        });
    }
}

/// 화이트보드에 이벤트를 전송합니다.
/// 커서 이동 이벤트는 쓰로틀링되어 4개 이벤트마다 1개만 전송됩니다.
pub fn update_whiteboard(k: String, e: CustomEvent) {
    let mut conns = CONNS.write().unwrap();
    let Some(conn) = conns.get_mut(&k) else {
        return;
    };
    match &e {
        CustomEvent::Cursor(cursor) => {
            // 커서 이벤트 카운터 증가
            conn.last_cursor_evt.c += 1;
            conn.last_cursor_evt.tm = Instant::now();
            if cursor.btns == 0 {
                // 버튼이 눌리지 않은 경우 (커서 이동만): 4개마다 1개만 전송 (대역폭 최적화)
                if conn.last_cursor_evt.c > 3 {
                    conn.last_cursor_evt.c = 0;
                    conn.last_cursor_evt.evt = None;
                    tx_send_event(conn, k, e);
                } else {
                    // 임시 저장했다가 타이머에서 나중에 전송
                    conn.last_cursor_evt.evt = Some(e);
                }
            } else {
                // 버튼이 눌린 경우: 이전에 대기 중인 이동 이벤트 먼저 전송
                if let Some(evt) = conn.last_cursor_evt.evt.take() {
                    tx_send_event(conn, k.clone(), evt);
                    conn.last_cursor_evt.c = 0;
                }
                // 클릭한 위치의 마지막 저장된 좌표를 사용하여 클릭 이벤트 전송
                let click_evt = CustomEvent::Cursor(Cursor {
                    x: conn.last_cursor_pos.0,
                    y: conn.last_cursor_pos.1,
                    argb: cursor.argb,
                    btns: cursor.btns,
                    text: cursor.text.clone(),
                });
                tx_send_event(conn, k, click_evt);
            }
        }
        _ => {
            // 다른 이벤트 타입은 바로 전송
            tx_send_event(conn, k, e);
        }
    }
}

/// 화이트보드 서버에 이벤트를 전송합니다.
/// 커서 이동 이벤트인 경우 마지막 위치를 업데이트합니다.
#[inline]
fn tx_send_event(conn: &mut Conn, k: String, event: CustomEvent) {
    // 커서 이동 이벤트인 경우 마지막 좌표 업데이트 (클릭 위치 결정용)
    if let CustomEvent::Cursor(cursor) = &event {
        if cursor.btns == 0 {
            conn.last_cursor_pos = (cursor.x, cursor.y);
        }
    }

    // 채널을 통해 이벤트 전송
    TX_WHITEBOARD.read().unwrap().as_ref().map(|tx| {
        allow_err!(tx.send((k, event)));
    });
}

/// 화이트보드 서버를 시작합니다.
/// IPC 채널을 통해 클라이언트와 통신합니다.
#[tokio::main(flavor = "current_thread")]
async fn start_whiteboard_() -> ResultType<()> {
    let mut tx_whiteboard = TX_WHITEBOARD.write().unwrap();
    // 이미 시작되었다면 중복 시작 방지
    if tx_whiteboard.is_some() {
        log::warn!("화이트보드가 이미 시작됨");
        return Ok(());
    }

    // 로그인 전 상태에서는 대기
    loop {
        if !crate::platform::is_prelogin() {
            break;
        }
        sleep(1.).await;
    }
    // 기존 화이트보드 서버에 연결 시도
    let mut stream = None;
    if let Ok(s) = ipc::connect(1000, "_whiteboard").await {
        stream = Some(s);
    } else {
        // 화이트보드 서버가 없으면 새로 시작
        #[allow(unused_mut)]
        #[allow(unused_assignments)]
        let mut args = vec!["--whiteboard"];
        #[allow(unused_mut)]
        #[cfg(target_os = "linux")]
        let mut user = None;

        let run_done;
        // 루트 권한인 경우 사용자 권한으로 프로세스 실행
        if crate::platform::is_root() {
            let mut res = Ok(None);
            for _ in 0..10 {
                #[cfg(not(any(target_os = "linux")))]
                {
                    log::debug!("화이트보드 시작");
                    res = crate::platform::run_as_user(args.clone());
                }
                #[cfg(target_os = "linux")]
                {
                    log::debug!("화이트보드 시작");
                    res = crate::platform::run_as_user(
                        args.clone(),
                        user.clone(),
                        None::<(&str, &str)>,
                    );
                }
                if res.is_ok() {
                    break;
                }
                log::error!("화이트보드 실행 실패: {res:?}");
                sleep(1.).await;
            }
            if let Some(task) = res? {
                CHILD_PROCESS.lock().unwrap().push(task);
            }
            run_done = true;
        } else {
            run_done = false;
        }
        // 루트가 아닌 경우 직접 프로세스 실행
        if !run_done {
            log::debug!("화이트보드 시작");
            CHILD_PROCESS.lock().unwrap().push(crate::run_me(args)?);
        }
        // 서버 시작 후 연결 시도 (최대 20회, 0.3초 간격)
        for _ in 0..20 {
            sleep(0.3).await;
            if let Ok(s) = ipc::connect(1000, "_whiteboard").await {
                stream = Some(s);
                break;
            }
        }
        if stream.is_none() {
            bail!("화이트보드 서버에 연결 실패");
        }
    }

    let mut stream = stream.ok_or(anyhow!("스트림 없음"))?;
    // 클라이언트로부터 이벤트를 받을 채널 생성
    let (tx, mut rx) = unbounded_channel();
    tx_whiteboard.replace(tx);
    drop(tx_whiteboard);
    // 함수 종료 시 자동으로 TX_WHITEBOARD를 정리
    let _call_on_ret = crate::common::SimpleCallOnReturn {
        b: true,
        f: Box::new(move || {
            let _ = TX_WHITEBOARD.write().unwrap().take();
        }),
    };

    // 타임아웃된 커서 이벤트를 전송하기 위한 타이머 (300ms 주기)
    let dur = tokio::time::Duration::from_millis(300);
    let mut timer = interval_at(tokio::time::Instant::now() + dur, dur);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // 메인 루프: 클라이언트 이벤트 수신 또는 타이머 처리
    loop {
        tokio::select! {
            // 클라이언트로부터 이벤트 수신
            res = rx.recv() => {
                match res {
                    Some(data) => {
                        // 종료 신호 받음
                        if matches!(data.1, CustomEvent::Exit) {
                            break;
                        } else {
                            // 이벤트를 서버로 전송하고 타이머 리셋
                            allow_err!(stream.send(&Data::Whiteboard(data)).await);
                            timer.reset();
                        }
                    }
                    None => {
                        bail!("채널 종료");
                    }
                }
            },
            // 타이머 만료: 쓰로틀링된 커서 이동 이벤트 전송
            _ = timer.tick() => {
                let mut conns = CONNS.write().unwrap();
                for (k, conn) in conns.iter_mut() {
                    // 300ms 이상 전송되지 않은 커서 이벤트가 있으면 강제 전송
                    if conn.last_cursor_evt.tm.elapsed().as_millis() > 300 {
                        if let Some(evt) = conn.last_cursor_evt.evt.take() {
                            allow_err!(stream.send(&Data::Whiteboard((k.clone(), evt))).await);
                            conn.last_cursor_evt.c = 0;
                        }
                    }
                }
            }
        }
    }
    // 종료 신호를 서버로 전송
    allow_err!(
        stream
            .send(&Data::Whiteboard(("".to_string(), CustomEvent::Exit)))
            .await
    );
    Ok(())
}
