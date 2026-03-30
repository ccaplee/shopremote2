use crate::hbbs_http::create_http_client_with_url;
use bytes::Bytes;
use hbb_common::{bail, config::Config, lazy_static, log, ResultType};
use reqwest::blocking::{Body, Client};
use scrap::record::RecordState;
use serde::Serialize;
use serde_json::Map;
use std::{
    fs::File,
    io::{prelude::*, SeekFrom},
    sync::{mpsc::Receiver, Arc, Mutex},
    time::{Duration, Instant},
};

// 파일 헤더 최대 크기
const MAX_HEADER_LEN: usize = 1024;
// 업로드 전송 시간 간격 (이 시간이 경과하면 전송)
const SHOULD_SEND_TIME: Duration = Duration::from_secs(1);
// 업로드 전송 크기 (이 크기 이상이면 전송, 1MB)
const SHOULD_SEND_SIZE: u64 = 1024 * 1024;

// 녹화 업로드 기능 활성화 상태
lazy_static::lazy_static! {
    static ref ENABLE: Arc<Mutex<bool>> = Default::default();
}

/// 녹화 업로드 기능이 활성화되어 있는지 확인
pub fn is_enable() -> bool {
    ENABLE.lock().unwrap().clone()
}

/// 녹화 파일 업로드 스레드 시작
/// RecordState 메시지를 수신하여 파일 업로드 처리
pub fn run(rx: Receiver<RecordState>) {
    std::thread::spawn(move || {
        let api_server = crate::get_api_server(
            Config::get_option("api-server"),
            Config::get_option("custom-rendezvous-server"),
        );
        // 이 URL은 TLS 연결 테스트 및 폴백 감지에 사용됨
        let login_option_url = format!("{}/api/login-options", &api_server);
        let client = create_http_client_with_url(&login_option_url);
        let mut uploader = RecordUploader {
            client,
            api_server,
            filepath: Default::default(),
            filename: Default::default(),
            upload_size: Default::default(),
            running: Default::default(),
            last_send: Instant::now(),
        };
        // 메인 루프: RecordState 메시지 처리
        loop {
            if let Err(e) = match rx.recv() {
                Ok(state) => match state {
                    // 새 녹화 파일 시작
                    RecordState::NewFile(filepath) => uploader.handle_new_file(filepath),
                    // 새 프레임 추가됨
                    RecordState::NewFrame => {
                        if uploader.running {
                            uploader.handle_frame(false)
                        } else {
                            Ok(())
                        }
                    }
                    // 파일 끝 표시 (최종 업로드)
                    RecordState::WriteTail => {
                        if uploader.running {
                            uploader.handle_tail()
                        } else {
                            Ok(())
                        }
                    }
                    // 파일 제거 알림
                    RecordState::RemoveFile => {
                        if uploader.running {
                            uploader.handle_remove()
                        } else {
                            Ok(())
                        }
                    }
                },
                Err(e) => {
                    log::trace!("업로드 스레드 중지: {}", e);
                    break;
                }
            } {
                uploader.running = false;
                log::error!("업로드 중지: {}", e);
            }
        }
    });
}

/// 녹화 파일 업로드를 관리하는 구조체
struct RecordUploader {
    // HTTP 클라이언트
    client: Client,
    // API 서버 주소
    api_server: String,
    // 현재 처리 중인 파일의 전체 경로
    filepath: String,
    // 현재 처리 중인 파일의 이름
    filename: String,
    // 이미 업로드한 파일 크기 (바이트)
    upload_size: u64,
    // 파일 업로드 진행 중 여부
    running: bool,
    // 마지막 업로드 시간
    last_send: Instant,
}

impl RecordUploader {
    /// HTTP POST를 통해 서버에 데이터 전송
    /// query: 쿼리 파라미터, body: 요청 본문
    fn send<Q, B>(&self, query: &Q, body: B) -> ResultType<()>
    where
        Q: Serialize + ?Sized,
        B: Into<Body>,
    {
        match self
            .client
            .post(format!("{}/api/record", self.api_server))
            .query(query)
            .body(body)
            .send()
        {
            Ok(resp) => {
                // 응답에서 에러 확인
                if let Ok(m) = resp.json::<Map<String, serde_json::Value>>() {
                    if let Some(e) = m.get("error") {
                        bail!(e.to_string());
                    }
                }
                Ok(())
            }
            Err(e) => bail!(e.to_string()),
        }
    }

    /// 새 녹화 파일 처리 시작
    /// 서버에 새 파일 알림 전송
    fn handle_new_file(&mut self, filepath: String) -> ResultType<()> {
        match std::path::PathBuf::from(&filepath).file_name() {
            Some(filename) => match filename.to_owned().into_string() {
                Ok(filename) => {
                    self.filename = filename.clone();
                    self.filepath = filepath.clone();
                    self.upload_size = 0;
                    self.running = true;
                    self.last_send = Instant::now();
                    // 서버에 새 파일 알림
                    self.send(&[("type", "new"), ("file", &filename)], Bytes::new())?;
                    Ok(())
                }
                Err(_) => bail!("파일명 파싱 실패:{:?}", filename),
            },
            None => bail!("파일 경로 파싱 실패:{}", filepath),
        }
    }

    /// 녹화 파일의 새로운 데이터를 서버에 업로드
    /// flush: true면 즉시 업로드, false면 조건 확인 후 업로드
    fn handle_frame(&mut self, flush: bool) -> ResultType<()> {
        // flush 아닐 경우 시간 조건 확인 (1초 이상 경과해야 전송)
        if !flush && self.last_send.elapsed() < SHOULD_SEND_TIME {
            return Ok(());
        }
        match File::open(&self.filepath) {
            Ok(mut file) => match file.metadata() {
                Ok(m) => {
                    let len = m.len();
                    // 파일 크기 변화 없으면 업로드 불필요
                    if len <= self.upload_size {
                        return Ok(());
                    }
                    // flush 아닐 경우 크기 조건 확인 (1MB 이상 증가해야 전송)
                    if !flush && len - self.upload_size < SHOULD_SEND_SIZE {
                        return Ok(());
                    }
                    // 마지막 업로드 위치부터 새로운 데이터 읽음
                    let mut buf = Vec::new();
                    match file.seek(SeekFrom::Start(self.upload_size)) {
                        Ok(_) => match file.read_to_end(&mut buf) {
                            Ok(length) => {
                                // 부분 데이터 업로드
                                self.send(
                                    &[
                                        ("type", "part"),
                                        ("file", &self.filename),
                                        ("offset", &self.upload_size.to_string()),
                                        ("length", &length.to_string()),
                                    ],
                                    buf,
                                )?;
                                self.upload_size = len;
                                self.last_send = Instant::now();
                                Ok(())
                            }
                            Err(e) => bail!(e.to_string()),
                        },
                        Err(e) => bail!(e.to_string()),
                    }
                }
                Err(e) => bail!(e.to_string()),
            },
            Err(e) => bail!(e.to_string()),
        }
    }

    /// 녹화 파일 업로드 완료 처리
    /// 최종 데이터 업로드 + 파일 헤더 전송
    fn handle_tail(&mut self) -> ResultType<()> {
        // 남은 데이터 모두 업로드 (flush=true)
        self.handle_frame(true)?;
        match File::open(&self.filepath) {
            Ok(mut file) => {
                // 파일 헤더 읽기 (최대 1KB)
                let mut buf = vec![0u8; MAX_HEADER_LEN];
                match file.read(&mut buf) {
                    Ok(length) => {
                        buf.truncate(length);
                        // 파일 끝 신호와 헤더 전송
                        self.send(
                            &[
                                ("type", "tail"),
                                ("file", &self.filename),
                                ("offset", "0"),
                                ("length", &length.to_string()),
                            ],
                            buf,
                        )?;
                        log::info!("업로드 성공, 파일: {}", self.filename);
                        Ok(())
                    }
                    Err(e) => bail!(e.to_string()),
                }
            }
            Err(e) => bail!(e.to_string()),
        }
    }

    /// 녹화 파일 제거 알림 전송
    fn handle_remove(&mut self) -> ResultType<()> {
        self.send(
            &[("type", "remove"), ("file", &self.filename)],
            Bytes::new(),
        )?;
        Ok(())
    }
}
