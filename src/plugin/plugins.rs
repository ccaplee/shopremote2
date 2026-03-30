use super::{desc::Desc, errno::*, *};
#[cfg(not(debug_assertions))]
use crate::common::is_server;
use crate::flutter;
use hbb_common::{
    bail,
    dlopen::symbor::Library,
    lazy_static, log,
    message_proto::{Message, Misc, PluginFailure, PluginRequest},
    ResultType,
};
use serde_derive::Serialize;
use std::{
    collections::{HashMap, HashSet},
    ffi::{c_char, c_void},
    path::Path,
    sync::{Arc, RwLock},
};

/// 플러그인 상태를 조회하는 메서드
pub const METHOD_HANDLE_STATUS: &[u8; 14] = b"handle_status\0";
/// 서명 검증을 처리하는 메서드
pub const METHOD_HANDLE_SIGNATURE_VERIFICATION: &[u8; 30] = b"handle_signature_verification\0";
/// UI 이벤트를 처리하는 메서드
const METHOD_HANDLE_UI: &[u8; 10] = b"handle_ui\0";
/// 피어 메시지를 처리하는 메서드
const METHOD_HANDLE_PEER: &[u8; 12] = b"handle_peer\0";
/// 이벤트 리스닝을 처리하는 메서드
pub const METHOD_HANDLE_LISTEN_EVENT: &[u8; 20] = b"handle_listen_event\0";

lazy_static::lazy_static! {
    /// 로드된 플러그인의 정보를 저장하는 맵
    static ref PLUGIN_INFO: Arc<RwLock<HashMap<String, PluginInfo>>> = Default::default();
    /// 로드된 플러그인의 네이티브 객체를 저장하는 맵
    static ref PLUGINS: Arc<RwLock<HashMap<String, Plugin>>> = Default::default();
}

/// 플러그인의 정보를 저장하는 구조체
pub(super) struct PluginInfo {
    /// 플러그인의 파일 경로
    pub path: String,
    /// 언로드되도록 표시되었는지 여부
    pub uninstalled: bool,
    /// 플러그인의 설명 정보
    pub desc: Desc,
}

/// 플러그인을 초기화하는 함수 타입
///
/// # 매개변수
/// * `data` - 초기화 데이터
type PluginFuncInit = extern "C" fn(data: *const InitData) -> PluginReturn;

/// 플러그인을 리셋하는 함수 타입
///
/// # 매개변수
/// * `data` - 초기화 데이터
type PluginFuncReset = extern "C" fn(data: *const InitData) -> PluginReturn;

/// 플러그인을 정리하는 함수 타입
type PluginFuncClear = extern "C" fn() -> PluginReturn;

/// 플러그인의 설명을 가져오는 함수 타입
/// 반환되는 메모리는 플러그인이 `libc::malloc`으로 할당하고 포인터를 반환해야 합니다
type PluginFuncDesc = extern "C" fn() -> *const c_char;

/// 메시지를 피어 또는 UI로 전송하는 콜백 함수 타입
/// peer, target, id는 UTF-8 널 종료 문자열입니다
///
/// # 매개변수
/// * `peer` - 피어 ID (UTF-8 널 종료 문자열)
/// * `target` - 메시지 대상 ("peer", "ui" 등)
/// * `id` - 플러그인 ID
/// * `content` - 메시지 내용
/// * `len` - 메시지 내용의 길이 (바이트)
type CallbackMsg = extern "C" fn(
    peer: *const c_char,
    target: *const c_char,
    id: *const c_char,
    content: *const c_void,
    len: usize,
) -> PluginReturn;

/// 설정 값을 가져오는 콜백 함수 타입
/// peer, key는 UTF-8 널 종료 문자열입니다
///
/// # 매개변수
/// * `peer` - 피어 ID (UTF-8 널 종료 문자열)
/// * `id` - 플러그인 ID
/// * `key` - 설정 키
///
/// # 반환값
/// UTF-8 널 종료 문자열 (호출자가 해제해야 함)
type CallbackGetConf =
    extern "C" fn(peer: *const c_char, id: *const c_char, key: *const c_char) -> *const c_char;

/// 로컬 피어 ID를 가져오는 콜백 함수 타입
///
/// # 반환값
/// UTF-8 널 종료 문자열 (호출자가 해제해야 함)
type CallbackGetId = extern "C" fn() -> *const c_char;

/// 로그를 작성하는 콜백 함수 타입
///
/// # 매개변수
/// * `level` - 로그 레벨 ("error", "warn", "info", "debug", "trace") (UTF-8 널 종료 문자열)
/// * `msg` - 로그 메시지 (UTF-8 널 종료 문자열)
type CallbackLog = extern "C" fn(level: *const c_char, msg: *const c_char);

/// RustDesk 코어로의 콜백 함수 타입
///
/// # 매개변수
/// * `method` - 콜백 메서드 이름
/// * `json` - 파라미터를 위한 JSON 데이터 (null이 아니어야 함)
/// * `raw` - 이 호출을 위한 바이너리 데이터 (선택사항, null 가능)
/// * `raw_len` - 바이너리 데이터의 길이 (raw 데이터가 있을 때만 유효)
type CallbackNative = extern "C" fn(
    method: *const c_char,
    json: *const c_char,
    raw: *const c_void,
    raw_len: usize,
) -> super::native::NativeReturnValue;

/// 플러그인의 메인 호출 함수 타입
///
/// # 매개변수
/// * `method` - 메서드 ("handle_ui" 또는 "handle_peer")
/// * `peer` - 피어 ID
/// * `args` - 메서드 인자
/// * `len` - 인자의 길이
type PluginFuncCall = extern "C" fn(
    method: *const c_char,
    peer: *const c_char,
    args: *const c_void,
    len: usize,
) -> PluginReturn;

/// 플러그인의 메인 호출 함수 타입 (출력 데이터 포함)
/// 주로 피어로부터의 메시지를 처리하고 응답을 보내기 위해 호출됩니다
///
/// # 매개변수
/// * `method` - 메서드 ("handle_ui" 또는 "handle_peer")
/// * `peer` - 피어 ID
/// * `args` - 메서드 인자
/// * `len` - 인자의 길이
/// * `out` - 출력 데이터 (플러그인이 `libc::malloc`으로 할당)
/// * `out_len` - 출력 데이터의 길이
type PluginFuncCallWithOutData = extern "C" fn(
    method: *const c_char,
    peer: *const c_char,
    args: *const c_void,
    len: usize,
    out: *mut *mut c_void,
    out_len: *mut usize,
) -> PluginReturn;

/// 플러그인 콜백들을 저장하는 구조체
/// msg: 메시지를 피어 또는 UI로 전송하는 콜백
/// get_conf: 설정 값을 가져오는 콜백
/// log: 로그를 작성하는 콜백
#[repr(C)]
#[derive(Copy, Clone)]
struct Callbacks {
    msg: CallbackMsg,
    get_conf: CallbackGetConf,
    get_id: CallbackGetId,
    log: CallbackLog,
    native: CallbackNative,
}

/// 플러그인 초기화 정보
#[derive(Serialize)]
#[repr(C)]
struct InitInfo {
    /// 이것이 서버인지 여부
    is_server: bool,
}

/// 플러그인 초기화 데이터
/// version: 플러그인의 버전 (null이 될 수 없음)
/// info: 로컬 피어 ID (null이 될 수 없음)
/// cbs: 콜백들
#[repr(C)]
struct InitData {
    /// 플러그인 버전
    version: *const c_char,
    /// 초기화 정보
    info: *const c_char,
    /// 콜백 함수들
    cbs: Callbacks,
}

impl Drop for InitData {
    fn drop(&mut self) {
        free_c_ptr(self.version as _);
        free_c_ptr(self.info as _);
    }
}

macro_rules! make_plugin {
    ($($field:ident : $tp:ty),+) => {
        #[allow(dead_code)]
        pub struct Plugin {
            _lib: Library,
            id: Option<String>,
            path: String,
            $($field: $tp),+
        }

        impl Plugin {
            fn new(path: &str) -> ResultType<Self> {
                let lib = match Library::open(path) {
                    Ok(lib) => lib,
                    Err(e) => {
                        bail!("Failed to load library {}, {}", path, e);
                    }
                };

                $(let $field = match unsafe { lib.symbol::<$tp>(stringify!($field)) } {
                        Ok(m) => {
                            *m
                        },
                        Err(e) => {
                            bail!("Failed to load {} func {}, {}", path, stringify!($field), e);
                        }
                    }
                ;)+

                Ok(Self {
                    _lib: lib,
                    id: None,
                    path: path.to_string(),
                    $( $field ),+
                })
            }

            fn desc(&self) -> ResultType<Desc> {
                let desc_ret = (self.desc)();
                let desc = Desc::from_cstr(desc_ret);
                free_c_ptr(desc_ret as _);
                desc
            }

            fn init(&self, data: &InitData, path: &str) -> ResultType<()> {
                let mut init_ret = (self.init)(data as _);
                if !init_ret.is_success() {
                    let (code, msg) = init_ret.get_code_msg(path);
                    bail!(
                        "Failed to init plugin {}, code: {}, msg: {}",
                        path,
                        code,
                        msg
                    );
                }
                Ok(())
            }

            fn clear(&self, id: &str) {
                let mut clear_ret = (self.clear)();
                if !clear_ret.is_success() {
                    let (code, msg) = clear_ret.get_code_msg(id);
                    log::error!(
                        "Failed to clear plugin {}, code: {}, msg: {}",
                        id,
                        code,
                        msg
                    );
                }
            }
        }

        impl Drop for Plugin {
            fn drop(&mut self) {
                let id = self.id.as_ref().unwrap_or(&self.path);
                self.clear(id);
            }
        }
    }
}

make_plugin!(
    init: PluginFuncInit,
    reset: PluginFuncReset,
    clear: PluginFuncClear,
    desc: PluginFuncDesc,
    call: PluginFuncCall,
    call_with_out_data: PluginFuncCallWithOutData
);

#[derive(Serialize)]
pub struct MsgListenEvent {
    pub event: String,
}

/// 플랫폼별 동적 라이브러리 파일 확장자
#[cfg(target_os = "windows")]
const DYLIB_SUFFIX: &str = ".dll";
#[cfg(target_os = "linux")]
const DYLIB_SUFFIX: &str = ".so";
#[cfg(target_os = "macos")]
const DYLIB_SUFFIX: &str = ".dylib";

/// 모든 플러그인을 로드합니다
/// 언로드 목록에 있는 플러그인은 건너뜁니다
pub(super) fn load_plugins(uninstalled_ids: &HashSet<String>) -> ResultType<()> {
    let plugins_dir = super::get_plugins_dir()?;
    if !plugins_dir.exists() {
        std::fs::create_dir_all(&plugins_dir)?;
    } else {
        for entry in std::fs::read_dir(plugins_dir)? {
            match entry {
                Ok(entry) => {
                    let plugin_dir = entry.path();
                    if plugin_dir.is_dir() {
                        if let Some(plugin_id) = plugin_dir.file_name().and_then(|f| f.to_str()) {
                            // 언로드되도록 표시된 플러그인은 무시
                            if uninstalled_ids.contains(plugin_id) {
                                log::debug!(
                                    "Ignore loading '{}' as it should be uninstalled",
                                    plugin_id
                                );
                                continue;
                            }
                            load_plugin_dir(&plugin_dir);
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to read plugins dir entry, {}", e);
                }
            }
        }
    }
    Ok(())
}

/// 플러그인 디렉토리에서 모든 플러그인 라이브러리를 로드합니다
fn load_plugin_dir(dir: &Path) {
    log::debug!("Begin load plugin dir: {}", dir.display());
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    if path.is_file() {
                        let filename = entry.file_name();
                        let filename = filename.to_str().unwrap_or("");
                        // "plugin_" 접두사와 올바른 확장자를 가진 파일만 로드
                        if filename.starts_with("plugin_") && filename.ends_with(DYLIB_SUFFIX) {
                            if let Some(path) = path.to_str() {
                                if let Err(e) = load_plugin_path(path) {
                                    log::error!("Failed to load plugin {}, {}", filename, e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!(
                        "Failed to read '{}' dir entry, {}",
                        dir.file_name().and_then(|f| f.to_str()).unwrap_or(""),
                        e
                    );
                }
            }
        }
    }
}

/// 플러그인을 언로드합니다
pub fn unload_plugin(id: &str) {
    log::info!("Plugin {} unloaded", id);
    PLUGINS.write().unwrap().remove(id);
}

/// 플러그인을 언로드되도록 표시합니다
pub(super) fn mark_uninstalled(id: &str, uninstalled: bool) {
    log::info!("Plugin {} uninstall", id);
    PLUGIN_INFO
        .write()
        .unwrap()
        .get_mut(id)
        .map(|info| info.uninstalled = uninstalled);
}

/// 플러그인을 다시 로드합니다 (언로드 후 로드)
pub fn reload_plugin(id: &str) -> ResultType<()> {
    let path = match PLUGIN_INFO.read().unwrap().get(id) {
        Some(plugin) => plugin.path.clone(),
        None => bail!("Plugin {} not found", id),
    };
    unload_plugin(id);
    load_plugin_path(&path)
}

/// 플러그인 파일 경로에서 플러그인을 로드합니다
fn load_plugin_path(path: &str) -> ResultType<()> {
    log::info!("Begin load plugin {}", path);

    // 동적 라이브러리 로드
    let plugin = Plugin::new(path)?;
    // 플러그인 설명 정보 조회
    let desc = plugin.desc()?;

    // TODO: 플러그인 검증
    // TODO: 플러그인 ID 확인 (다른 플러그인의 ID를 사용하지 않는지 확인)

    let id = desc.meta().id.clone();
    let plugin_info = PluginInfo {
        path: path.to_string(),
        uninstalled: false,
        desc: desc.clone(),
    };
    PLUGIN_INFO.write().unwrap().insert(id.clone(), plugin_info);

    // 초기화 정보 생성
    let init_info = serde_json::to_string(&InitInfo {
        is_server: super::is_server_running(),
    })?;
    let init_data = InitData {
        version: str_to_cstr_ret(crate::VERSION),
        info: str_to_cstr_ret(&init_info) as _,
        cbs: Callbacks {
            msg: callback_msg::cb_msg,
            get_conf: config::cb_get_conf,
            get_id: config::cb_get_local_peer_id,
            log: super::plog::plugin_log,
            native: super::native::cb_native_data,
        },
    };
    // 초기화 실패해도 플러그인을 로드합니다 (UI에 표시되어야 하므로)
    if let Err(e) = plugin.init(&init_data, path) {
        log::error!("Failed to init plugin '{}', {}", desc.meta().id, e);
    }

    // 서버 모드에서 매니저 설정에 플러그인 추가
    if super::is_server_running() {
        super::config::ManagerConfig::add_plugin(&desc.meta().id)?;
    }

    // UI 업데이트
    // UI가 아직 준비되지 않았을 수 있으므로, 준비되면 다시 업데이트합니다
    reload_ui(&desc, None);

    // 플러그인 등록
    PLUGINS.write().unwrap().insert(id.clone(), plugin);

    log::info!("Plugin {} loaded, {}", id, path);
    Ok(())
}

/// 특정 채널에 모든 플러그인의 UI를 동기화합니다
pub fn sync_ui(sync_to: String) {
    for plugin in PLUGIN_INFO.read().unwrap().values() {
        reload_ui(&plugin.desc, Some(&sync_to));
    }
}

/// 플러그인을 로드합니다
#[inline]
pub fn load_plugin(id: &str) -> ResultType<()> {
    load_plugin_dir(&super::get_plugin_dir(id)?);
    Ok(())
}

/// 플러그인 이벤트를 처리합니다
#[inline]
fn handle_event(method: &[u8], id: &str, peer: &str, event: &[u8]) -> ResultType<()> {
    let mut peer: String = peer.to_owned();
    peer.push('\0');
    plugin_call(id, method, &peer, event)
}

/// 플러그인 메서드를 호출합니다
pub fn plugin_call(id: &str, method: &[u8], peer: &str, event: &[u8]) -> ResultType<()> {
    let mut ret = plugin_call_get_return(id, method, peer, event)?;
    if ret.is_success() {
        Ok(())
    } else {
        let (code, msg) = ret.get_code_msg(id);
        bail!(
            "Failed to handle plugin event, id: {}, method: {}, code: {}, msg: {}",
            id,
            std::string::String::from_utf8(method.to_vec()).unwrap_or_default(),
            code,
            msg
        );
    }
}

/// 플러그인 메서드를 호출하고 반환값을 얻습니다
#[inline]
pub fn plugin_call_get_return(
    id: &str,
    method: &[u8],
    peer: &str,
    event: &[u8],
) -> ResultType<PluginReturn> {
    match PLUGINS.read().unwrap().get(id) {
        Some(plugin) => Ok((plugin.call)(
            method.as_ptr() as _,
            peer.as_ptr() as _,
            event.as_ptr() as _,
            event.len(),
        )),
        None => bail!("Plugin {} not found", id),
    }
}

/// UI 이벤트를 플러그인에 전달합니다
#[inline]
pub fn handle_ui_event(id: &str, peer: &str, event: &[u8]) -> ResultType<()> {
    handle_event(METHOD_HANDLE_UI, id, peer, event)
}

/// 서버 이벤트를 플러그인에 전달합니다
#[inline]
pub fn handle_server_event(id: &str, peer: &str, event: &[u8]) -> ResultType<()> {
    handle_event(METHOD_HANDLE_PEER, id, peer, event)
}

/// 리스닝 이벤트를 플러그인에 전달합니다
fn _handle_listen_event(event: String, peer: String) {
    let mut plugins = Vec::new();
    // 이벤트를 리스닝하는 플러그인 찾기
    for info in PLUGIN_INFO.read().unwrap().values() {
        if info.desc.listen_events().contains(&event.to_string()) {
            plugins.push(info.desc.meta().id.clone());
        }
    }

    if plugins.is_empty() {
        return;
    }

    if let Ok(evt) = serde_json::to_string(&MsgListenEvent {
        event: event.clone(),
    }) {
        let mut evt_bytes = evt.as_bytes().to_vec();
        evt_bytes.push(0);
        let mut peer: String = peer.to_owned();
        peer.push('\0');
        // 각 플러그인에 이벤트 전달
        for id in plugins {
            match PLUGINS.read().unwrap().get(&id) {
                Some(plugin) => {
                    let mut ret = (plugin.call)(
                        METHOD_HANDLE_LISTEN_EVENT.as_ptr() as _,
                        peer.as_ptr() as _,
                        evt_bytes.as_ptr() as _,
                        evt_bytes.len(),
                    );
                    if !ret.is_success() {
                        let (code, msg) = ret.get_code_msg(&id);
                        log::error!(
                            "Failed to handle plugin listen event, id: {}, event: {}, code: {}, msg: {}",
                            id,
                            event,
                            code,
                            msg
                        );
                    }
                }
                None => {
                    log::error!("Plugin {} not found when handle_listen_event", id);
                }
            }
        }
    }
}

/// 리스닝 이벤트를 별도 스레드에서 처리합니다
#[inline]
pub fn handle_listen_event(event: String, peer: String) {
    std::thread::spawn(|| _handle_listen_event(event, peer));
}

#[inline]
pub fn handle_client_event(id: &str, peer: &str, event: &[u8]) -> Message {
    let mut peer: String = peer.to_owned();
    peer.push('\0');
    match PLUGINS.read().unwrap().get(id) {
        Some(plugin) => {
            let mut out = std::ptr::null_mut();
            let mut out_len: usize = 0;
            let mut ret = (plugin.call_with_out_data)(
                METHOD_HANDLE_PEER.as_ptr() as _,
                peer.as_ptr() as _,
                event.as_ptr() as _,
                event.len(),
                &mut out as _,
                &mut out_len as _,
            );
            if ret.is_success() {
                let msg = make_plugin_request(id, out, out_len);
                free_c_ptr(out as _);
                msg
            } else {
                let (code, msg) = ret.get_code_msg(id);
                if code > ERR_RUSTDESK_HANDLE_BASE && code < ERR_PLUGIN_HANDLE_BASE {
                    log::debug!(
                        "Plugin {} failed to handle client event, code: {}, msg: {}",
                        id,
                        code,
                        msg
                    );
                    let name = match PLUGIN_INFO.read().unwrap().get(id) {
                        Some(plugin) => &plugin.desc.meta().name,
                        None => "???",
                    }
                    .to_owned();
                    match code {
                        ERR_CALL_NOT_SUPPORTED_METHOD => {
                            make_plugin_failure(id, &name, "Plugin method is not supported")
                        }
                        ERR_CALL_INVALID_ARGS => {
                            make_plugin_failure(id, &name, "Plugin arguments is invalid")
                        }
                        _ => make_plugin_failure(id, &name, &msg),
                    }
                } else {
                    log::error!(
                        "Plugin {} failed to handle client event, code: {}, msg: {}",
                        id,
                        code,
                        msg
                    );
                    let msg = make_plugin_request(id, out, out_len);
                    free_c_ptr(out as _);
                    msg
                }
            }
        }
        None => make_plugin_failure(id, "", "Plugin not found"),
    }
}

/// 플러그인 요청 메시지를 생성합니다
fn make_plugin_request(id: &str, content: *const c_void, len: usize) -> Message {
    let mut misc = Misc::new();
    misc.set_plugin_request(PluginRequest {
        id: id.to_owned(),
        content: unsafe { std::slice::from_raw_parts(content as *const u8, len) }
            .clone()
            .into(),
        ..Default::default()
    });
    let mut msg_out = Message::new();
    msg_out.set_misc(misc);
    msg_out
}

/// 플러그인 실패 메시지를 생성합니다
fn make_plugin_failure(id: &str, name: &str, msg: &str) -> Message {
    let mut misc = Misc::new();
    misc.set_plugin_failure(PluginFailure {
        id: id.to_owned(),
        name: name.to_owned(),
        msg: msg.to_owned(),
        ..Default::default()
    });
    let mut msg_out = Message::new();
    msg_out.set_misc(misc);
    msg_out
}

/// 플러그인의 UI를 Flutter에 다시 로드합니다
fn reload_ui(desc: &Desc, sync_to: Option<&str>) {
    for (location, ui) in desc.location().ui.iter() {
        if let Ok(ui) = serde_json::to_string(&ui) {
            let make_event = |ui: &str| {
                let mut m = HashMap::new();
                m.insert("name", MSG_TO_UI_TYPE_PLUGIN_RELOAD);
                m.insert("id", &desc.meta().id);
                m.insert("location", &location);
                // UI에서 "location"과 플러그인 설명에 의존하지 마세요.
                // UI 필드를 전송하여 유효성을 보장합니다.
                m.insert("ui", ui);
                serde_json::to_string(&m).unwrap_or("".to_owned())
            };
            match sync_to {
                Some(channel) => {
                    // 특정 채널로 이벤트 전송
                    let _res = flutter::push_global_event(channel, make_event(&ui));
                }
                None => {
                    // 위치 문자열 파싱
                    let v: Vec<&str> = location.split('|').collect();
                    // 첫 번째 요소는 "client" 또는 "host"
                    // 두 번째 요소는 "main", "remote", "cm", "file transfer", "port forward"
                    if v.len() >= 2 {
                        let available_channels = flutter::get_global_event_channels();
                        if available_channels.contains(&v[1]) {
                            let _res = flutter::push_global_event(v[1], make_event(&ui));
                        }
                    }
                }
            }
        }
    }
}

/// 모든 플러그인 정보의 Arc를 반환합니다
pub(super) fn get_plugin_infos() -> Arc<RwLock<HashMap<String, PluginInfo>>> {
    PLUGIN_INFO.clone()
}

/// 플러그인의 설정 정보를 가져옵니다
pub(super) fn get_desc_conf(id: &str) -> Option<super::desc::Config> {
    PLUGIN_INFO
        .read()
        .unwrap()
        .get(id)
        .map(|info| info.desc.config().clone())
}

/// 플러그인의 버전을 가져옵니다
pub(super) fn get_version(id: &str) -> Option<String> {
    PLUGIN_INFO
        .read()
        .unwrap()
        .get(id)
        .map(|info| info.desc.meta().version.clone())
}
