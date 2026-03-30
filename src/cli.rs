use crate::client::*;
use async_trait::async_trait;
use hbb_common::{
    config::PeerConfig,
    config::READ_TIMEOUT,
    futures::{SinkExt, StreamExt},
    log,
    message_proto::*,
    protobuf::Message as _,
    rendezvous_proto::ConnType,
    tokio::{self, sync::mpsc},
    Stream,
};
use std::sync::{Arc, RwLock};

/// CLI를 위한 세션 구조체
/// 원격 연결의 상태와 인증 정보를 관리하는 클라이언트 세션
#[derive(Clone)]
pub struct Session {
    /// 세션의 고유 식별자 (원격 peer ID)
    id: String,
    /// 로그인 설정과 상태를 관리하는 핸들러
    lc: Arc<RwLock<LoginConfigHandler>>,
    /// 메시지를 전송하기 위한 채널 송신자
    sender: mpsc::UnboundedSender<Data>,
    /// 원격 연결을 위한 비밀번호
    password: String,
}

impl Session {
    /// 새로운 CLI 세션을 생성합니다
    /// 비밀번호가 없으면 사용자 입력을 받습니다
    pub fn new(id: &str, sender: mpsc::UnboundedSender<Data>) -> Self {
        let mut password = "".to_owned();
        // 저장된 비밀번호가 없으면 사용자에게 입력 받기
        if PeerConfig::load(id).password.is_empty() {
            password = rpassword::prompt_password("Enter password: ").unwrap();
        }
        let session = Self {
            id: id.to_owned(),
            sender,
            password,
            lc: Default::default(),
        };
        // 로그인 핸들러 초기화 (포트 포워딩 연결 타입으로 설정)
        session.lc.write().unwrap().initialize(
            id.to_owned(),
            ConnType::PORT_FORWARD,
            None,
            false,
            None,
            None,
        );
        session
    }
}

/// Interface 특성 구현: 세션의 인증과 이벤트 처리 로직
#[async_trait]
impl Interface for Session {
    /// 로그인 설정 핸들러를 반환합니다
    fn get_login_config_handler(&self) -> Arc<RwLock<LoginConfigHandler>> {
        return self.lc.clone();
    }

    /// 사용자에게 메시지를 표시하거나 입력을 요청합니다
    /// msgtype: 메시지 종류 (input-password, re-input-password, error 등)
    fn msgbox(&self, msgtype: &str, title: &str, text: &str, link: &str) {
        match msgtype {
            "input-password" => {
                // 저장된 비밀번호를 사용하여 로그인
                self.sender
                    .send(Data::Login((self.password.clone(), true)))
                    .ok();
            }
            "re-input-password" => {
                // 비밀번호 재입력 요청
                log::error!("{}: {}", title, text);
                match rpassword::prompt_password("Enter password: ") {
                    Ok(password) => {
                        let login_data = Data::Login((password, true));
                        self.sender.send(login_data).ok();
                    }
                    Err(e) => {
                        log::error!("reinput password failed, {:?}", e);
                    }
                }
            }
            msg if msg.contains("error") => {
                // 에러 메시지 처리
                log::error!("{}: {}: {}", msgtype, title, text);
            }
            _ => {
                // 일반 정보 메시지 처리
                log::info!("{}: {}: {}", msgtype, title, text);
            }
        }
    }

    /// 로그인 에러를 처리합니다
    fn handle_login_error(&self, err: &str) -> bool {
        handle_login_error(self.lc.clone(), err, self)
    }

    /// peer 정보를 수신하고 처리합니다
    fn handle_peer_info(&self, pi: PeerInfo) {
        self.lc.write().unwrap().handle_peer_info(&pi);
    }

    /// 비밀번호 해시 인증을 처리합니다
    async fn handle_hash(&self, pass: &str, hash: Hash, peer: &mut Stream) {
        log::info!(
            "password={}",
            hbb_common::password_security::temporary_password()
        );
        handle_hash(self.lc.clone(), &pass, hash, self, peer).await;
    }

    /// UI에서 로그인 정보를 처리합니다
    async fn handle_login_from_ui(
        &self,
        os_username: String,
        os_password: String,
        password: String,
        remember: bool,
        peer: &mut Stream,
    ) {
        handle_login_from_ui(
            self.lc.clone(),
            os_username,
            os_password,
            password,
            remember,
            peer,
        )
        .await;
    }

    /// 연결 지연 시간을 테스트합니다
    async fn handle_test_delay(&self, t: TestDelay, peer: &mut Stream) {
        handle_test_delay(t, peer).await;
    }

    /// 데이터를 전송합니다
    fn send(&self, data: Data) {
        self.sender.send(data).ok();
    }
}

/// 원격 연결을 테스트하는 비동기 함수
/// 해시 인증 메시지를 받을 때까지 연결을 유지합니다
#[tokio::main(flavor = "current_thread")]
pub async fn connect_test(id: &str, key: String, token: String) {
    let (sender, mut receiver) = mpsc::unbounded_channel::<Data>();
    let handler = Session::new(&id, sender);
    match crate::client::Client::start(id, &key, &token, ConnType::PORT_FORWARD, handler).await {
        Err(err) => {
            log::error!("Failed to connect {}: {}", &id, err);
        }
        Ok((mut stream, direct)) => {
            log::info!("direct: {}", direct);
            // 스트림에서 메시지를 수신하는 루프
            loop {
                tokio::select! {
                    res = hbb_common::timeout(READ_TIMEOUT, stream.next()) => match res {
                        Err(_) => {
                            // 타임아웃 발생 시 연결 종료
                            log::error!("Timeout");
                            break;
                        }
                        Ok(Some(Ok(bytes))) => {
                            if let Ok(msg_in) = Message::parse_from_bytes(&bytes) {
                                match msg_in.union {
                                    Some(message::Union::Hash(hash)) => {
                                        // 해시 메시지 수신 시 테스트 완료
                                        log::info!("Got hash");
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// 포트 포워딩을 시작하는 비동기 함수
/// 로컬 포트를 원격 호스트의 포트로 포워딩합니다
#[tokio::main(flavor = "current_thread")]
pub async fn start_one_port_forward(
    id: String,
    port: i32,
    remote_host: String,
    remote_port: i32,
    key: String,
    token: String,
) {
    // 렌데즈부 서버와 NAT 타입 테스트
    crate::common::test_rendezvous_server();
    crate::common::test_nat_type();
    let (sender, mut receiver) = mpsc::unbounded_channel::<Data>();
    let handler = Session::new(&id, sender);
    // 포트 포워딩 리스너 시작
    if let Err(err) = crate::port_forward::listen(
        handler.id.clone(),
        handler.password.clone(),
        port,
        handler.clone(),
        receiver,
        &key,
        &token,
        handler.lc.clone(),
        remote_host,
        remote_port,
    )
    .await
    {
        log::error!("Failed to listen on {}: {}", port, err);
    }
    log::info!("port forward (:{}) exit", port);
}
