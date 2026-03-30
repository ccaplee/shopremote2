/// DBus 기반 URL 핸들러
///
/// 주의:
/// Linux에서는 DBus를 사용하여 여러 rustdesk 프로세스와 통신
/// [Flutter]: Linux에서 uni 링크 처리
use dbus::blocking::Connection;
use dbus_crossroads::{Crossroads, IfaceBuilder};
use hbb_common::log;
#[cfg(feature = "flutter")]
use std::collections::HashMap;
use std::{error::Error, fmt, time::Duration};

/// DBus 서비스 이름
const DBUS_NAME: &str = "org.rustdesk.rustdesk";
/// DBus 객체 경로
const DBUS_PREFIX: &str = "/dbus";
/// 새 연결 메서드 이름
const DBUS_METHOD_NEW_CONNECTION: &str = "NewConnection";
/// DBus 메서드 파라미터: 연결 ID
const DBUS_METHOD_NEW_CONNECTION_ID: &str = "id";
/// DBus 메서드 반환값 이름
const DBUS_METHOD_RETURN: &str = "ret";
/// DBus 메서드 성공 반환값
const DBUS_METHOD_RETURN_SUCCESS: &str = "ok";
/// DBus 통신 타임아웃 (5초)
const DBUS_TIMEOUT: Duration = Duration::from_secs(5);

/// DBus 오류를 나타내는 구조체
#[derive(Debug)]
struct DbusError(String);

impl fmt::Display for DbusError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RustDesk DBus 오류: {}", self.0)
    }
}

impl Error for DbusError {}

/// DBus 서버에서 새 연결 호출
///
/// 주의:
/// CLI에서 테스트하는 방법:
/// - dbus-send 명령어 사용:
/// `dbus-send --session --print-reply --dest=org.rustdesk.rustdesk /dbus org.rustdesk.rustdesk.NewConnection string:'PEER_ID'`
///
/// uni_links: 처리할 URI 링크
pub fn invoke_new_connection(uni_links: String) -> Result<(), Box<dyn Error>> {
    log::info!("DBus 서비스 시작 (uni)");
    let conn = Connection::new_session()?;
    let proxy = conn.with_proxy(DBUS_NAME, DBUS_PREFIX, DBUS_TIMEOUT);
    let (ret,): (String,) =
        proxy.method_call(DBUS_NAME, DBUS_METHOD_NEW_CONNECTION, (uni_links,))?;
    if ret != DBUS_METHOD_RETURN_SUCCESS {
        log::error!("DBus 서버에 새 연결 호출 오류");
        return Err(Box::new(DbusError("성공하지 못함".to_string())));
    }
    Ok(())
}

/// DBus 서버 시작
///
/// 주의:
/// 이 함수는 현재 스레드를 차단하여 DBus 서버를 제공함
/// 따라서 DBus 서버 전용 스레드를 생성하여 호출하는 것이 적합함
pub fn start_dbus_server() -> Result<(), Box<dyn Error>> {
    let conn: Connection = Connection::new_session()?;
    let _ = conn.request_name(DBUS_NAME, false, true, false)?;
    let mut cr = Crossroads::new();
    let token = cr.register(DBUS_NAME, handle_client_message);
    cr.insert(DBUS_PREFIX, &[token], ());
    cr.serve(&conn)?;
    Ok(())
}

/// DBus 클라이언트 메시지 처리
/// 새 연결 요청을 처리하고 Flutter 앱에 이벤트 전송
fn handle_client_message(builder: &mut IfaceBuilder<()>) {
    // 새 연결 DBus 메서드 등록
    builder.method(
        DBUS_METHOD_NEW_CONNECTION,
        (DBUS_METHOD_NEW_CONNECTION_ID,),
        (DBUS_METHOD_RETURN,),
        move |_, _, (_uni_links,): (String,)| {
            // Flutter 플랫폼에서 URL 스킴 이벤트 처리
            #[cfg(feature = "flutter")]
            {
                use crate::flutter;
                // URL 링크를 포함한 이벤트 데이터 생성
                let data = HashMap::from([
                    ("name", "on_url_scheme_received"),
                    ("url", _uni_links.as_str()),
                ]);
                let event = serde_json::ser::to_string(&data).unwrap_or("".to_string());
                // Flutter 앱의 전역 이벤트 스트림으로 이벤트 전송
                match crate::flutter::push_global_event(flutter::APP_TYPE_MAIN, event) {
                    None => log::error!("메인 이벤트 스트림을 찾지 못함"),
                    Some(false) => {
                        log::error!("DBus 메시지를 Flutter 전역 DBus 스트림에 추가 실패")
                    }
                    Some(true) => {}
                }
            }
            return Ok((DBUS_METHOD_RETURN_SUCCESS.to_string(),));
        },
    );
}
