use std::sync::{Arc, Mutex, RwLock};

#[cfg(any(
    target_os = "windows",
    all(target_os = "macos", feature = "unix-file-copy-paste")
))]
use hbb_common::ResultType;
#[cfg(any(target_os = "windows", feature = "unix-file-copy-paste"))]
use hbb_common::{allow_err, log};
use hbb_common::{
    lazy_static,
    tokio::sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        Mutex as TokioMutex,
    },
};
use serde_derive::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(any(
    target_os = "windows",
    all(target_os = "macos", feature = "unix-file-copy-paste")
))]
pub mod context_send;
pub mod platform;
#[cfg(any(
    target_os = "windows",
    all(target_os = "macos", feature = "unix-file-copy-paste")
))]
pub use context_send::*;

#[cfg(target_os = "windows")]
const ERR_CODE_SERVER_FUNCTION_NONE: u32 = 0x00000001;
#[cfg(target_os = "windows")]
const ERR_CODE_INVALID_PARAMETER: u32 = 0x00000002;
#[cfg(target_os = "windows")]
const ERR_CODE_SEND_MSG: u32 = 0x00000003;

#[cfg(any(
    target_os = "windows",
    all(target_os = "macos", feature = "unix-file-copy-paste")
))]
pub(crate) use platform::create_cliprdr_context;

/// 클립보드 작업의 진행도를 나타내는 구조체입니다.
pub struct ProgressPercent {
    /// 진행도 비율 (0.0 ~ 100.0)
    pub percent: f64,
    /// 작업이 취소되었는지 여부
    pub is_canceled: bool,
    /// 작업이 실패했는지 여부
    pub is_failed: bool,
}

// TODO: 유닉스 파일 복사/붙여넣기에서는 필요 없으므로 이 트레이트는 제거될 수 있습니다.
/// 원격 shopremote2 클라이언트에서 클립보드 파일을 처리할 수 있는 능력을 제공합니다.
///
/// # 참고
/// 실제로는 사용 가능한 클립보드 파일 서비스를 구현하기 위해 2개의 부분이 필요하지만,
/// 여기에는 RPC 서버 부분만 포함되어 있습니다.
/// 로컬 리스너와 전송 부분은 플랫폼 별로 너무 구체적이어서 유형 클래스로 래핑할 수 없습니다.
pub trait CliprdrServiceContext: Send + Sync {
    /// 중지 상태로 설정합니다.
    fn set_is_stopped(&mut self) -> Result<(), CliprdrError>;
    /// 클립보드의 내용을 비웁니다.
    fn empty_clipboard(&mut self, conn_id: i32) -> Result<bool, CliprdrError>;
    /// 클립보드 RPC에 대한 서버로 실행합니다.
    fn server_clip_file(&mut self, conn_id: i32, msg: ClipboardFile) -> Result<(), CliprdrError>;
    /// 붙여넣기 작업의 진행 상황을 가져옵니다.
    fn get_progress_percent(&self) -> Option<ProgressPercent>;
    /// 붙여넣기 작업을 취소합니다.
    fn cancel(&mut self);
}

/// 클립보드 작업 중에 발생할 수 있는 모든 에러 타입을 정의합니다.
#[derive(Error, Debug)]
pub enum CliprdrError {
    #[error("invalid cliprdr name")]
    CliprdrName,
    #[error("failed to init cliprdr")]
    CliprdrInit,
    #[error("cliprdr out of memory")]
    CliprdrOutOfMemory,
    #[error("cliprdr internal error")]
    ClipboardInternalError,
    #[error("cliprdr occupied")]
    ClipboardOccupied,
    #[error("conversion failure")]
    ConversionFailure,
    #[error("failure to read clipboard")]
    OpenClipboard,
    #[error("failure to read file metadata or content, path: {path}, err: {err}")]
    FileError { path: String, err: std::io::Error },
    #[error("invalid request: {description}")]
    InvalidRequest { description: String },
    #[error("common request: {description}")]
    CommonError { description: String },
    #[error("unknown cliprdr error")]
    Unknown(u32),
}

/// 클립보드 파일 작업을 위한 메시지 타입을 정의합니다.
/// 원격 클라이언트와 서버 간의 클립보드 데이터 교환에 사용됩니다.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "t", content = "c")]
pub enum ClipboardFile {
    NotifyCallback {
        r#type: String,
        title: String,
        text: String,
    },
    MonitorReady,
    FormatList {
        format_list: Vec<(i32, String)>,
    },
    FormatListResponse {
        msg_flags: i32,
    },
    FormatDataRequest {
        requested_format_id: i32,
    },
    FormatDataResponse {
        msg_flags: i32,
        format_data: Vec<u8>,
    },
    FileContentsRequest {
        stream_id: i32,
        list_index: i32,
        dw_flags: i32,
        n_position_low: i32,
        n_position_high: i32,
        cb_requested: i32,
        have_clip_data_id: bool,
        clip_data_id: i32,
    },
    FileContentsResponse {
        msg_flags: i32,
        stream_id: i32,
        requested_data: Vec<u8>,
    },
    TryEmpty,
    Files {
        files: Vec<(String, u64)>,
    },
}

/// 메시지 채널을 나타내는 구조체입니다.
/// 각 피어(클라이언트)와의 클립보드 메시지 통신을 관리합니다.
struct MsgChannel {
    /// 원격 피어의 식별자
    peer_id: String,
    /// 연결 ID
    conn_id: i32,
    /// 메시지 송신자
    #[allow(dead_code)]
    sender: UnboundedSender<ClipboardFile>,
    /// 메시지 수신자 (tokio 뮤텍스로 보호됨)
    receiver: Arc<TokioMutex<UnboundedReceiver<ClipboardFile>>>,
}

/// 전역 메시지 채널 목록을 관리합니다. 모든 피어의 메시지 채널을 저장합니다.
lazy_static::lazy_static! {
    /// 활성 메시지 채널의 벡터 (읽기-쓰기 잠금으로 보호됨)
    static ref VEC_MSG_CHANNEL: RwLock<Vec<MsgChannel>> = Default::default();
    /// 클라이언트 연결 ID를 증가시키는 카운터
    static ref CLIENT_CONN_ID_COUNTER: Mutex<i32> = Mutex::new(0);
}

impl ClipboardFile {
    /// 클립보드 작업 중단이 허용되는 메시지인지 확인합니다.
    /// 특정 메시지 타입에서만 작업을 안전하게 중단할 수 있습니다.
    pub fn is_stopping_allowed(&self) -> bool {
        matches!(
            self,
            ClipboardFile::MonitorReady
                | ClipboardFile::FormatList { .. }
                | ClipboardFile::FormatDataRequest { .. }
        )
    }

    /// 작업 시작 메시지인지 확인합니다.
    /// 모니터링 시작이나 포맷 리스트 제공은 새로운 작업의 시작을 나타냅니다.
    pub fn is_beginning_message(&self) -> bool {
        matches!(
            self,
            ClipboardFile::MonitorReady | ClipboardFile::FormatList { .. }
        )
    }
}

/// 주어진 피어 ID에 해당하는 연결 ID를 조회합니다.
pub fn get_client_conn_id(peer_id: &str) -> Option<i32> {
    VEC_MSG_CHANNEL
        .read()
        .unwrap()
        .iter()
        .find(|x| x.peer_id == peer_id)
        .map(|x| x.conn_id)
}

/// 새로운 연결 ID를 생성하고 반환합니다. 원자적으로 증가시킵니다.
fn get_conn_id() -> i32 {
    let mut lock = CLIENT_CONN_ID_COUNTER.lock().unwrap();
    *lock += 1;
    *lock
}

/// 클라이언트용 클립보드 수신기를 가져옵니다.
/// 기존 채널이 있으면 반환하고, 없으면 새로 생성합니다.
/// 반환값: (연결 ID, 메시지 수신기)
pub fn get_rx_cliprdr_client(
    peer_id: &str,
) -> (i32, Arc<TokioMutex<UnboundedReceiver<ClipboardFile>>>) {
    let mut lock = VEC_MSG_CHANNEL.write().unwrap();
    match lock.iter().find(|x| x.peer_id == peer_id) {
        Some(msg_channel) => (msg_channel.conn_id, msg_channel.receiver.clone()),
        None => {
            let (sender, receiver) = unbounded_channel();
            let receiver = Arc::new(TokioMutex::new(receiver));
            let receiver2 = receiver.clone();
            let conn_id = get_conn_id();
            let msg_channel = MsgChannel {
                peer_id: peer_id.to_owned(),
                conn_id,
                sender,
                receiver,
            };
            lock.push(msg_channel);
            (conn_id, receiver2)
        }
    }
}

/// 서버용 클립보드 수신기를 가져옵니다.
/// 연결 ID로 기존 채널을 찾으며, 없으면 새로 생성합니다.
pub fn get_rx_cliprdr_server(conn_id: i32) -> Arc<TokioMutex<UnboundedReceiver<ClipboardFile>>> {
    let mut lock = VEC_MSG_CHANNEL.write().unwrap();
    match lock.iter().find(|x| x.conn_id == conn_id) {
        Some(msg_channel) => msg_channel.receiver.clone(),
        None => {
            let (sender, receiver) = unbounded_channel();
            let receiver = Arc::new(TokioMutex::new(receiver));
            let receiver2 = receiver.clone();
            let msg_channel = MsgChannel {
                peer_id: "".to_string(),
                conn_id,
                sender,
                receiver,
            };
            lock.push(msg_channel);
            receiver2
        }
    }
}

/// 연결 ID로 메시지 채널을 제거합니다.
/// 클라이언트 연결을 종료할 때 호출되어야 합니다.
pub fn remove_channel_by_conn_id(conn_id: i32) {
    let mut lock = VEC_MSG_CHANNEL.write().unwrap();
    if let Some(index) = lock.iter().position(|x| x.conn_id == conn_id) {
        lock.remove(index);
    }
}

/// 클립보드 데이터를 전송합니다.
/// Windows에서는 특정 연결로만 전송하고, Unix에서는 conn_id가 0이면 모든 연결로 전송합니다.
#[cfg(any(target_os = "windows", feature = "unix-file-copy-paste"))]
#[inline]
pub fn send_data(conn_id: i32, data: ClipboardFile) -> Result<(), CliprdrError> {
    #[cfg(target_os = "windows")]
    return send_data_to_channel(conn_id, data);
    #[cfg(not(target_os = "windows"))]
    if conn_id == 0 {
        let _ = send_data_to_all(data);
        Ok(())
    } else {
        send_data_to_channel(conn_id, data)
    }
}

/// 특정 연결로 데이터를 전송합니다.
/// 주어진 연결 ID에 해당하는 채널을 찾아 데이터를 전송합니다.
#[inline]
#[cfg(any(target_os = "windows", feature = "unix-file-copy-paste"))]
fn send_data_to_channel(conn_id: i32, data: ClipboardFile) -> Result<(), CliprdrError> {
    if let Some(msg_channel) = VEC_MSG_CHANNEL
        .read()
        .unwrap()
        .iter()
        .find(|x| x.conn_id == conn_id)
    {
        msg_channel
            .sender
            .send(data)
            .map_err(|e| CliprdrError::CommonError {
                description: e.to_string(),
            })
    } else {
        Err(CliprdrError::InvalidRequest {
            description: "conn_id not found".to_string(),
        })
    }
}

/// 특정 연결을 제외한 모든 연결로 데이터를 전송합니다 (Windows 전용).
/// 자신을 제외한 다른 모든 클라이언트에게 클립보드 업데이트를 브로드캐스트합니다.
#[inline]
#[cfg(target_os = "windows")]
pub fn send_data_exclude(conn_id: i32, data: ClipboardFile) {
    // 에러 처리가 필요한지 확인하기 위해 더 많은 테스트가 필요합니다.
    for msg_channel in VEC_MSG_CHANNEL.read().unwrap().iter() {
        if msg_channel.conn_id != conn_id {
            allow_err!(msg_channel.sender.send(data.clone()));
        }
    }
}

/// 모든 메시지 채널로 데이터를 전송합니다.
#[inline]
#[cfg(feature = "unix-file-copy-paste")]
fn send_data_to_all(data: ClipboardFile) {
    // 에러 처리가 필요한지 확인하기 위해 더 많은 테스트가 필요합니다.
    for msg_channel in VEC_MSG_CHANNEL.read().unwrap().iter() {
        allow_err!(msg_channel.sender.send(data.clone()));
    }
}

#[cfg(test)]
mod tests {
    // #[test]
    // fn test_cliprdr_run() {
    //     super::cliprdr_run();
    // }
}
