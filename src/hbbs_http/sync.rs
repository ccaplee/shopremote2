use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

#[cfg(not(any(target_os = "ios")))]
use crate::{ui_interface::get_builtin_option, Connection};
use hbb_common::{
    config::{self, keys, Config, LocalConfig},
    log,
    tokio::{self, sync::broadcast, time::Instant},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// 하트비트 전송 간격
const TIME_HEARTBEAT: Duration = Duration::from_secs(15);
// 시스템 정보 업로드 타임아웃 (2분)
const UPLOAD_SYSINFO_TIMEOUT: Duration = Duration::from_secs(120);
// 연결 확인 주기 (3초)
const TIME_CONN: Duration = Duration::from_secs(3);

// iOS 제외 플랫폼에서만 동기화 실행
#[cfg(not(any(target_os = "ios")))]
lazy_static::lazy_static! {
    // 연결 해제 신호 브로드캐스트 채널
    static ref SENDER : Mutex<broadcast::Sender<Vec<i32>>> = Mutex::new(start_hbbs_sync());
    // Pro 버전 여부
    static ref PRO: Arc<Mutex<bool>> = Default::default();
}

/// 하트비트/동기화 시작
#[cfg(not(any(target_os = "ios")))]
pub fn start() {
    let _sender = SENDER.lock().unwrap();
}

/// 연결 해제 신호 수신자 생성
#[cfg(not(target_os = "ios"))]
pub fn signal_receiver() -> broadcast::Receiver<Vec<i32>> {
    SENDER.lock().unwrap().subscribe()
}

/// HBBS 동기화 시작 - 브로드캐스트 채널 초기화
#[cfg(not(any(target_os = "ios")))]
fn start_hbbs_sync() -> broadcast::Sender<Vec<i32>> {
    let (tx, _rx) = broadcast::channel::<Vec<i32>>(16);
    // 비동기 루프를 별도 스레드에서 실행
    std::thread::spawn(move || start_hbbs_sync_async());
    return tx;
}

/// 서버로부터 받은 전략 옵션
/// 설정 옵션과 추가 정보를 포함
#[derive(Debug, Serialize, Deserialize)]
pub struct StrategyOptions {
    // 설정 옵션 맵 (설정 이름 -> 값)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub config_options: HashMap<String, String>,
    // 추가 정보
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extra: HashMap<String, String>,
}

/// 시스템 정보 업로드 추적 구조체
/// 마지막 성공적인 업로드 상태를 기록
struct InfoUploaded {
    // 업로드 완료 여부
    uploaded: bool,
    // 업로드한 서버 URL
    url: String,
    // 마지막 업로드 시간
    last_uploaded: Option<Instant>,
    // 기기 ID
    id: String,
    // 기기 사용자 이름
    username: Option<String>,
}

impl Default for InfoUploaded {
    /// 초기 상태: 업로드되지 않음
    fn default() -> Self {
        Self {
            uploaded: false,
            url: "".to_owned(),
            last_uploaded: None,
            id: "".to_owned(),
            username: None,
        }
    }
}

impl InfoUploaded {
    /// 업로드 성공 상태 생성
    fn uploaded(url: String, id: String, username: String) -> Self {
        Self {
            uploaded: true,
            url,
            last_uploaded: None,
            id,
            username: Some(username),
        }
    }
}

/// 비동기 하트비트/동기화 메인 루프
/// 주기적으로 서버와 통신하여 상태 업데이트 및 설정 동기화
#[cfg(not(any(target_os = "ios")))]
#[tokio::main(flavor = "current_thread")]
async fn start_hbbs_sync_async() {
    // 3초마다 실행되는 주기 타이머
    let mut interval = crate::shopremote2_interval(tokio::time::interval_at(
        Instant::now() + TIME_CONN,
        TIME_CONN,
    ));
    let mut last_sent: Option<Instant> = None;
    let mut info_uploaded = InfoUploaded::default();
    let mut sysinfo_ver = "".to_owned();

    // 메인 동기화 루프
    loop {
        tokio::select! {
            _ = interval.tick() => {
                // 하트비트 URL과 기기 ID 조회
                let url = heartbeat_url();
                let id = Config::get_id();
                if url.is_empty() {
                    *PRO.lock().unwrap() = false;
                    continue;
                }
                // 서비스 중지 옵션 확인
                if config::option2bool("stop-service", &Config::get_option("stop-service")) {
                    continue;
                }
                // 활성 연결 목록 조회
                let conns = Connection::alive_conns();
                // URL 또는 기기 ID가 변경되었으면 다시 업로드
                if info_uploaded.uploaded && (url != info_uploaded.url || id != info_uploaded.id) {
                    info_uploaded.uploaded = false;
                    *PRO.lock().unwrap() = false;
                }

                // Windows 플랫폼 주의사항:
                // 사용자명이 비어있을 때도 시스템 정보를 업로드해야 함
                // (로그인 전에는 사용자명이 비어있을 수 있음)
                // 참고: https://github.com/rustdesk/rustdesk/discussions/8031
                //
                // 업로드 후에도 사용자명을 확인해야 하는 이유:
                // 1. 로그인 중에는 사용자명이 비어있을 수 있고, 나중에 얻을 수 있으므로
                //    다시 업로드해야 함
                // 2. 업로드 후에 사용자명이 변경되었을 수 있으므로 다시 업로드해야 함
                //
                // Windows의 세션은 재시작 전 마지막 사용자 세션으로 전환되므로
                // 로그인 전에도 사용자명을 얻을 수 있을 수 있음.
                // 하지만 재시작 후에는 사용자명을 얻을 수 없을 수도 있음.
                let mut v = crate::get_sysinfo();
                let sys_username = v["username"].as_str().unwrap_or_default().to_string();
                // 사용자명 비교는 Windows 전용이지만, 다른 플랫폼에서도 일관성을 위해 확인
                let need_upload = (!info_uploaded.uploaded || info_uploaded.username.as_ref() != Some(&sys_username)) &&
                    info_uploaded.last_uploaded.map(|x| x.elapsed() >= UPLOAD_SYSINFO_TIMEOUT).unwrap_or(true);
                if need_upload {
                    v["version"] = json!(crate::VERSION);
                    v["id"] = json!(id);
                    v["uuid"] = json!(crate::encode64(hbb_common::get_uuid()));
                    let ab_name = Config::get_option(keys::OPTION_PRESET_ADDRESS_BOOK_NAME);
                    if !ab_name.is_empty() {
                        v[keys::OPTION_PRESET_ADDRESS_BOOK_NAME] = json!(ab_name);
                    }
                    let ab_tag = Config::get_option(keys::OPTION_PRESET_ADDRESS_BOOK_TAG);
                    if !ab_tag.is_empty() {
                        v[keys::OPTION_PRESET_ADDRESS_BOOK_TAG] = json!(ab_tag);
                    }
                    let ab_alias = Config::get_option(keys::OPTION_PRESET_ADDRESS_BOOK_ALIAS);
                    if !ab_alias.is_empty() {
                        v[keys::OPTION_PRESET_ADDRESS_BOOK_ALIAS] = json!(ab_alias);
                    }
                    let ab_password = Config::get_option(keys::OPTION_PRESET_ADDRESS_BOOK_PASSWORD);
                    if !ab_password.is_empty() {
                        v[keys::OPTION_PRESET_ADDRESS_BOOK_PASSWORD] = json!(ab_password);
                    }
                    let ab_note = Config::get_option(keys::OPTION_PRESET_ADDRESS_BOOK_NOTE);
                    if !ab_note.is_empty() {
                        v[keys::OPTION_PRESET_ADDRESS_BOOK_NOTE] = json!(ab_note);
                    }
                    let username = get_builtin_option(keys::OPTION_PRESET_USERNAME);
                    if !username.is_empty() {
                        v[keys::OPTION_PRESET_USERNAME] = json!(username);
                    }
                    let strategy_name = get_builtin_option(keys::OPTION_PRESET_STRATEGY_NAME);
                    if !strategy_name.is_empty() {
                        v[keys::OPTION_PRESET_STRATEGY_NAME] = json!(strategy_name);
                    }
                    let device_group_name = get_builtin_option(keys::OPTION_PRESET_DEVICE_GROUP_NAME);
                    if !device_group_name.is_empty() {
                        v[keys::OPTION_PRESET_DEVICE_GROUP_NAME] = json!(device_group_name);
                    }
                    let device_username = Config::get_option(keys::OPTION_PRESET_DEVICE_USERNAME);
                    if !device_username.is_empty() {
                        v["username"] = json!(device_username);
                    }
                    let device_name = Config::get_option(keys::OPTION_PRESET_DEVICE_NAME);
                    if !device_name.is_empty() {
                        v["hostname"] = json!(device_name);
                    }
                    let note = Config::get_option(keys::OPTION_PRESET_NOTE);
                    if !note.is_empty() {
                        v[keys::OPTION_PRESET_NOTE] = json!(note);
                    }
                    let v = v.to_string();
                    let mut hash = "".to_owned();
                    if crate::is_public(&url) {
                        use sha2::{Digest, Sha256};
                        let mut hasher = Sha256::new();
                        hasher.update(url.as_bytes());
                        hasher.update(&v.as_bytes());
                        let res = hasher.finalize();
                        hash = hbb_common::base64::encode(&res[..]);
                        let old_hash = config::Status::get("sysinfo_hash");
                        let ver = config::Status::get("sysinfo_ver"); // sysinfo_ver is the version of sysinfo on server's side
                        if hash == old_hash {
                            // When the api doesn't exist, Ok("") will be returned in test.
                            let samever = match crate::post_request(url.replace("heartbeat", "sysinfo_ver"), "".to_owned(), "").await {
                                Ok(x)  => {
                                    sysinfo_ver = x.clone();
                                    *PRO.lock().unwrap() = true;
                                    x == ver
                                }
                                _ => {
                                    false // to make sure Pro can be assigned in below post for old
                                            // hbbs pro not supporting sysinfo_ver, use false for ensuring
                                }
                            };
                            if samever {
                                info_uploaded = InfoUploaded::uploaded(url.clone(), id.clone(), sys_username);
                                log::info!("sysinfo not changed, skip upload");
                                continue;
                            }
                        }
                    }
                    match crate::post_request(url.replace("heartbeat", "sysinfo"), v, "").await {
                        Ok(x)  => {
                            if x == "SYSINFO_UPDATED" {
                                info_uploaded = InfoUploaded::uploaded(url.clone(), id.clone(), sys_username);
                                log::info!("sysinfo updated");
                                if !hash.is_empty() {
                                    config::Status::set("sysinfo_hash", hash);
                                    config::Status::set("sysinfo_ver", sysinfo_ver.clone());
                                }
                                *PRO.lock().unwrap() = true;
                            } else if x == "ID_NOT_FOUND" {
                                info_uploaded.last_uploaded = None; // next heartbeat will upload sysinfo again
                            } else {
                                info_uploaded.last_uploaded = Some(Instant::now());
                            }
                        }
                        _ => {
                            info_uploaded.last_uploaded = Some(Instant::now());
                        }
                    }
                }
                if conns.is_empty() && last_sent.map(|x| x.elapsed() < TIME_HEARTBEAT).unwrap_or(false) {
                    continue;
                }
                last_sent = Some(Instant::now());
                let mut v = Value::default();
                v["id"] = json!(id);
                v["uuid"] = json!(crate::encode64(hbb_common::get_uuid()));
                v["ver"] = json!(hbb_common::get_version_number(crate::VERSION));
                if !conns.is_empty() {
                    v["conns"] = json!(conns);
                }
                let modified_at = LocalConfig::get_option("strategy_timestamp").parse::<i64>().unwrap_or(0);
                v["modified_at"] = json!(modified_at);
                if let Ok(s) = crate::post_request(url.clone(), v.to_string(), "").await {
                    if let Ok(mut rsp) = serde_json::from_str::<HashMap::<&str, Value>>(&s) {
                        if rsp.remove("sysinfo").is_some() {
                            info_uploaded.uploaded = false;
                            config::Status::set("sysinfo_hash", "".to_owned());
                            log::info!("sysinfo required to forcely update");
                        }
                        if let Some(conns)  = rsp.remove("disconnect") {
                                if let Ok(conns) = serde_json::from_value::<Vec<i32>>(conns) {
                                    SENDER.lock().unwrap().send(conns).ok();
                                }
                        }
                        if let Some(rsp_modified_at) = rsp.remove("modified_at") {
                            if let Ok(rsp_modified_at) = serde_json::from_value::<i64>(rsp_modified_at) {
                                if rsp_modified_at != modified_at {
                                    LocalConfig::set_option("strategy_timestamp".to_string(), rsp_modified_at.to_string());
                                }
                            }
                        }
                        if let Some(strategy) = rsp.remove("strategy") {
                            if let Ok(strategy) = serde_json::from_value::<StrategyOptions>(strategy) {
                                log::info!("strategy updated");
                                handle_config_options(strategy.config_options);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 하트비트 엔드포인트 URL 생성
/// 공개 서버면 빈 문자열 반환
fn heartbeat_url() -> String {
    let url = crate::common::get_api_server(
        Config::get_option("api-server"),
        Config::get_option("custom-rendezvous-server"),
    );
    if url.is_empty() || crate::is_public(&url) {
        return "".to_owned();
    }
    format!("{}/api/heartbeat", url)
}

/// 서버로부터 받은 설정 옵션 적용
fn handle_config_options(config_options: HashMap<String, String>) {
    let mut options = Config::get_options();
    config_options
        .iter()
        .map(|(k, v)| {
            if v.is_empty() {
                // 값이 비어있으면 옵션 제거
                options.remove(k);
            } else {
                // 값이 있으면 옵션 업데이트
                options.insert(k.to_string(), v.to_string());
            }
        })
        .count();
    Config::set_options(options);
}

/// Pro 버전 여부 확인
#[allow(unused)]
#[cfg(not(any(target_os = "ios")))]
pub fn is_pro() -> bool {
    PRO.lock().unwrap().clone()
}
