use super::HbbHttpResponse;
use crate::hbbs_http::create_http_client_with_url;
use hbb_common::{config::LocalConfig, log, ResultType};
use reqwest::blocking::Client;
use serde_derive::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use url::Url;

// OIDC 세션 전역 싱글톤 인스턴스
lazy_static::lazy_static! {
    static ref OIDC_SESSION: Arc<RwLock<OidcSession>> = Arc::new(RwLock::new(OidcSession::new()));
}

// 쿼리 간격 (초 단위)
const QUERY_INTERVAL_SECS: f32 = 1.0;
// 쿼리 타임아웃 (초 단위, 3분)
const QUERY_TIMEOUT_SECS: u64 = 60 * 3;

// 계정 인증 요청 상태 메시지
const REQUESTING_ACCOUNT_AUTH: &str = "Requesting account auth";
// 계정 인증 대기 상태 메시지
const WAITING_ACCOUNT_AUTH: &str = "Waiting account auth";
// 계정 인증 로그인 상태 메시지
const LOGIN_ACCOUNT_AUTH: &str = "Login account auth";

#[derive(Deserialize, Clone, Debug)]
pub struct OidcAuthUrl {
    code: String,
    url: Url,
}

/// 기기 정보를 나타내는 구조체
/// 운영 체제, 타입(브라우저/클라이언트), 기기 이름 등의 정보를 포함
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct DeviceInfo {
    /// 운영 체제 (Linux, Windows, Android ...)
    #[serde(default)]
    pub os: String,

    /// 기기 타입: `browser` 또는 `client`
    #[serde(default)]
    pub r#type: String,

    /// 기기 이름 (rustdesk 클라이언트에서 가져옴) 또는
    /// 브라우저 정보(이름 + 버전) (브라우저에서 가져옴)
    #[serde(default)]
    pub name: String,
}

/// 로그인 기기 화이트리스트 항목
/// IP 주소 또는 기기 UUID와 관련 정보를 저장
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WhitelistItem {
    // IP 주소 또는 기기 UUID
    data: String,
    // 기기 정보
    info: DeviceInfo,
    // 만료 시간 (타임스탬프)
    exp: u64,
}

/// 사용자 정보 구조체
/// 사용자 설정, 로그인 기기 화이트리스트, 기타 정보를 포함
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserInfo {
    // 사용자 설정 (flatten으로 병합됨)
    #[serde(default, flatten)]
    pub settings: UserSettings,
    // 로그인이 허용된 기기 목록
    #[serde(default)]
    pub login_device_whitelist: Vec<WhitelistItem>,
    // 기타 사용자 정보 (key-value 맵)
    #[serde(default)]
    pub other: HashMap<String, String>,
}

/// 사용자 설정 구조체
/// 이메일 인증 및 알림 설정을 포함
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserSettings {
    // 이메일 인증 여부
    #[serde(default)]
    pub email_verification: bool,
    // 이메일 알람 알림 수신 여부
    #[serde(default)]
    pub email_alarm_notification: bool,
}

/// 사용자 상태 열거형
/// 사용자의 계정 활성화 상태를 나타냄
#[derive(Debug, Clone, Copy, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(i64)]
pub enum UserStatus {
    // 계정 비활성화됨
    Disabled = 0,
    // 정상 활성 계정
    Normal = 1,
    // 이메일 미확인 계정
    Unverified = -1,
}

/// 사용자 정보 페이로드 구조체
/// 인증 응답에 포함되는 사용자 상세 정보
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPayload {
    // 사용자 이름/로그인 ID
    pub name: String,
    // 사용자 표시 이름 (선택사항)
    #[serde(default)]
    pub display_name: Option<String>,
    // 사용자 아바타 URL (선택사항)
    #[serde(default)]
    pub avatar: Option<String>,
    // 사용자 이메일 주소 (선택사항)
    #[serde(default)]
    pub email: Option<String>,
    // 사용자 메모 (선택사항)
    #[serde(default)]
    pub note: Option<String>,
    // 사용자 상태
    #[serde(default)]
    pub status: UserStatus,
    // 사용자 상세 정보
    pub info: UserInfo,
    // 관리자 권한 여부
    #[serde(default)]
    pub is_admin: bool,
    // 제3자 인증 타입 (Google, GitHub 등, 선택사항)
    #[serde(default)]
    pub third_auth_type: Option<String>,
}

/// 인증 응답 본문 구조체
/// 서버로부터의 인증 성공 응답에 포함되는 데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthBody {
    // 액세스 토큰 (세션 인증에 사용)
    pub access_token: String,
    // 응답 타입 (예: "access_token")
    pub r#type: String,
    // 2단계 인증 타입 (TOTP, SMS 등)
    #[serde(default)]
    pub tfa_type: String,
    // 2단계 인증 시크릿 (선택사항)
    #[serde(default)]
    pub secret: String,
    // 인증된 사용자 정보
    pub user: UserPayload,
}

/// OIDC 인증 세션 구조체
/// OAuth2/OIDC 인증 플로우의 상태와 클라이언트를 관리
pub struct OidcSession {
    // HTTP 클라이언트 (선택사항)
    client: Option<Client>,
    // 현재 인증 상태 메시지
    state_msg: &'static str,
    // 오류 메시지 (실패 시)
    failed_msg: String,
    // 인증 코드와 URL
    code_url: Option<OidcAuthUrl>,
    // 인증 성공 후의 응답 본문
    auth_body: Option<AuthBody>,
    // 쿼리 계속 여부 (false면 타임아웃 또는 사용자 취소)
    keep_querying: bool,
    // 인증 작업 실행 중 여부
    running: bool,
    // 쿼리 타임아웃
    query_timeout: Duration,
}

/// 인증 결과 구조체
/// 클라이언트에게 반환되는 인증 상태와 결과 정보
#[derive(Serialize)]
pub struct AuthResult {
    // 현재 인증 상태 메시지
    pub state_msg: String,
    // 오류 메시지 (실패 시)
    pub failed_msg: String,
    // 인증 URL (사용자가 방문해야 함)
    pub url: Option<String>,
    // 인증 성공 시의 응답 본문
    pub auth_body: Option<AuthBody>,
}

impl Default for UserStatus {
    fn default() -> Self {
        UserStatus::Normal
    }
}

impl OidcSession {
    /// OIDC 세션 새 인스턴스 생성
    fn new() -> Self {
        Self {
            client: None,
            state_msg: REQUESTING_ACCOUNT_AUTH,
            failed_msg: "".to_owned(),
            code_url: None,
            auth_body: None,
            keep_querying: false,
            running: false,
            query_timeout: Duration::from_secs(QUERY_TIMEOUT_SECS),
        }
    }

    /// HTTP 클라이언트가 없으면 생성하고 설정함
    /// 주어진 API 서버 URL을 기반으로 TLS 설정을 감지하여 클라이언트를 생성
    fn ensure_client(api_server: &str) {
        let mut write_guard = OIDC_SESSION.write().unwrap();
        if write_guard.client.is_none() {
            // 이 URL은 서버의 적절한 TLS 구현을 감지하기 위해 사용됨
            let login_option_url = format!("{}/api/login-options", &api_server);
            let client = create_http_client_with_url(&login_option_url);
            write_guard.client = Some(client);
        }
    }

    /// OIDC 인증 요청을 서버에 전송
    /// 인증 URL과 코드를 받아옴
    fn auth(
        api_server: &str,
        op: &str,
        id: &str,
        uuid: &str,
    ) -> ResultType<HbbHttpResponse<OidcAuthUrl>> {
        Self::ensure_client(api_server);
        let resp = if let Some(client) = &OIDC_SESSION.read().unwrap().client {
            client
                .post(format!("{}/api/oidc/auth", api_server))
                .json(&serde_json::json!({
                    "op": op,
                    "id": id,
                    "uuid": uuid,
                    "deviceInfo": crate::ui_interface::get_login_device_info(),
                }))
                .send()?
        } else {
            hbb_common::bail!("HTTP 클라이언트가 초기화되지 않음");
        };
        let status = resp.status();
        match resp.try_into() {
            Ok(v) => Ok(v),
            Err(err) => {
                hbb_common::bail!("HTTP 상태: {}, 오류: {}", status, err);
            }
        }
    }

    /// OIDC 인증 코드로 사용자 정보를 조회
    /// 인증 성공 시 액세스 토큰과 사용자 정보를 받아옴
    fn query(
        api_server: &str,
        code: &str,
        id: &str,
        uuid: &str,
    ) -> ResultType<HbbHttpResponse<AuthBody>> {
        let url = Url::parse_with_params(
            &format!("{}/api/oidc/auth-query", api_server),
            &[("code", code), ("id", id), ("uuid", uuid)],
        )?;
        Self::ensure_client(api_server);
        if let Some(client) = &OIDC_SESSION.read().unwrap().client {
            Ok(client.get(url).send()?.try_into()?)
        } else {
            hbb_common::bail!("HTTP 클라이언트가 초기화되지 않음")
        }
    }

    /// 세션 상태를 초기값으로 리셋
    fn reset(&mut self) {
        self.state_msg = REQUESTING_ACCOUNT_AUTH;
        self.failed_msg = "".to_owned();
        self.keep_querying = true;
        self.running = false;
        self.code_url = None;
        self.auth_body = None;
    }

    /// 인증 작업 시작 전 준비
    fn before_task(&mut self) {
        self.reset();
        self.running = true;
    }

    /// 인증 작업 종료 후 정리
    fn after_task(&mut self) {
        self.running = false;
    }

    /// 지정된 시간(초) 동안 스레드 일시 중지
    fn sleep(secs: f32) {
        std::thread::sleep(std::time::Duration::from_secs_f32(secs));
    }

    /// OIDC 인증 작업 스레드 메인 루프
    /// 인증 요청 -> 대기 -> 쿼리 반복 -> 성공/실패 처리
    fn auth_task(api_server: String, op: String, id: String, uuid: String, remember_me: bool) {
        // 단계 1: 인증 요청 - 사용자에게 보여줄 인증 URL 획득
        let auth_request_res = Self::auth(&api_server, &op, &id, &uuid);
        log::info!("OIDC 인증 요청 결과: {:?}", &auth_request_res);
        let code_url = match auth_request_res {
            Ok(HbbHttpResponse::<_>::Data(code_url)) => code_url,
            Ok(HbbHttpResponse::<_>::Error(err)) => {
                OIDC_SESSION
                    .write()
                    .unwrap()
                    .set_state(REQUESTING_ACCOUNT_AUTH, err);
                return;
            }
            Ok(_) => {
                OIDC_SESSION
                    .write()
                    .unwrap()
                    .set_state(REQUESTING_ACCOUNT_AUTH, "잘못된 인증 응답".to_owned());
                return;
            }
            Err(err) => {
                OIDC_SESSION
                    .write()
                    .unwrap()
                    .set_state(REQUESTING_ACCOUNT_AUTH, err.to_string());
                return;
            }
        };

        // 단계 2: 사용자가 브라우저에서 인증을 완료할 때까지 대기
        OIDC_SESSION
            .write()
            .unwrap()
            .set_state(WAITING_ACCOUNT_AUTH, "".to_owned());
        OIDC_SESSION.write().unwrap().code_url = Some(code_url.clone());

        // 단계 3: 정기적으로 서버에 쿼리하여 사용자 인증 완료 확인
        let begin = Instant::now();
        let query_timeout = OIDC_SESSION.read().unwrap().query_timeout;
        while OIDC_SESSION.read().unwrap().keep_querying && begin.elapsed() < query_timeout {
            match Self::query(&api_server, &code_url.code, &id, &uuid) {
                Ok(HbbHttpResponse::<_>::Data(auth_body)) => {
                    // 인증 성공 - 액세스 토큰과 사용자 정보 획득
                    if auth_body.r#type == "access_token" {
                        // 선택사항: "기억하기" 선택 시 로컬에 저장
                        if remember_me {
                            LocalConfig::set_option(
                                "access_token".to_owned(),
                                auth_body.access_token.clone(),
                            );
                            LocalConfig::set_option(
                                "user_info".to_owned(),
                                serde_json::json!({
                                    "name": auth_body.user.name,
                                    "display_name": auth_body.user.display_name,
                                    "avatar": auth_body.user.avatar,
                                    "status": auth_body.user.status
                                })
                                .to_string(),
                            );
                        }
                    }
                    OIDC_SESSION
                        .write()
                        .unwrap()
                        .set_state(LOGIN_ACCOUNT_AUTH, "".to_owned());
                    OIDC_SESSION.write().unwrap().auth_body = Some(auth_body);
                    return;
                }
                Ok(HbbHttpResponse::<_>::Error(err)) => {
                    // 아직 인증이 완료되지 않은 경우는 무시하고 계속 쿼리
                    if err.contains("No authed oidc is found") {
                        // 무시, 계속 쿼리
                    } else {
                        // 다른 오류는 사용자에게 보고
                        OIDC_SESSION
                            .write()
                            .unwrap()
                            .set_state(WAITING_ACCOUNT_AUTH, err);
                        return;
                    }
                }
                Ok(_) => {
                    // 무시
                }
                Err(err) => {
                    log::trace!("OIDC 쿼리 실패 {}", err);
                    // 무시
                }
            }
            Self::sleep(QUERY_INTERVAL_SECS);
        }

        // 단계 4: 타임아웃 처리
        if begin.elapsed() >= query_timeout {
            OIDC_SESSION
                .write()
                .unwrap()
                .set_state(WAITING_ACCOUNT_AUTH, "timeout".to_owned());
        }

        // keep_querying이 false인 경우는 별도로 처리할 필요 없음
    }

    /// 세션 상태 메시지 업데이트
    fn set_state(&mut self, state_msg: &'static str, failed_msg: String) {
        self.state_msg = state_msg;
        self.failed_msg = failed_msg;
    }

    /// 진행 중인 인증 작업이 완료될 때까지 대기
    fn wait_stop_querying() {
        let wait_secs = 0.3;
        while OIDC_SESSION.read().unwrap().running {
            Self::sleep(wait_secs);
        }
    }

    /// 계정 인증 시작 (비동기)
    /// 새로운 스레드에서 인증 작업을 실행하고 즉시 반환
    pub fn account_auth(
        api_server: String,
        op: String,
        id: String,
        uuid: String,
        remember_me: bool,
    ) {
        // 이전 인증 요청 취소
        Self::auth_cancel();
        // 이전 인증 작업이 완료될 때까지 대기
        Self::wait_stop_querying();
        // 새로운 인증 작업 준비
        OIDC_SESSION.write().unwrap().before_task();
        // 별도 스레드에서 인증 작업 실행
        std::thread::spawn(move || {
            Self::auth_task(api_server, op, id, uuid, remember_me);
            OIDC_SESSION.write().unwrap().after_task();
        });
    }

    /// 현재 세션의 인증 결과를 내부 포맷으로 반환
    fn get_result_(&self) -> AuthResult {
        AuthResult {
            state_msg: self.state_msg.to_string(),
            failed_msg: self.failed_msg.clone(),
            url: self.code_url.as_ref().map(|x| x.url.to_string()),
            auth_body: self.auth_body.clone(),
        }
    }

    /// 진행 중인 인증 작업 취소
    pub fn auth_cancel() {
        OIDC_SESSION.write().unwrap().keep_querying = false;
    }

    /// 현재 인증 결과를 조회 (공개 API)
    pub fn get_result() -> AuthResult {
        OIDC_SESSION.read().unwrap().get_result_()
    }
}
