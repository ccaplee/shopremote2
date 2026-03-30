// 플러그인 매니저의 역할:
// 1. 플러그인 업데이트 확인
// 2. 플러그인 설치 또는 언로드

use super::{desc::Meta as PluginMeta, ipc::InstallStatus, *};
use crate::flutter;
use crate::hbbs_http::create_http_client;
use hbb_common::{allow_err, bail, log, tokio, toml};
use serde_derive::{Deserialize, Serialize};
use serde_json;
use std::{
    collections::{HashMap, HashSet},
    fs::{read_to_string, remove_dir_all, OpenOptions},
    io::Write,
    sync::{Arc, Mutex},
};

/// UI로 전송할 플러그인 목록 메시지 타입
const MSG_TO_UI_PLUGIN_MANAGER_LIST: &str = "plugin_list";
/// UI로 전송할 플러그인 설치 메시지 타입
const MSG_TO_UI_PLUGIN_MANAGER_INSTALL: &str = "plugin_install";
/// UI로 전송할 플러그인 언로드 메시지 타입
const MSG_TO_UI_PLUGIN_MANAGER_UNINSTALL: &str = "plugin_uninstall";

/// IPC 플러그인 채널 접미사
const IPC_PLUGIN_POSTFIX: &str = "_plugin";

/// 현재 플랫폼에 대한 플러그인 플랫폼 타입
#[cfg(target_os = "windows")]
const PLUGIN_PLATFORM: &str = "windows";
#[cfg(target_os = "linux")]
const PLUGIN_PLATFORM: &str = "linux";
#[cfg(target_os = "macos")]
const PLUGIN_PLATFORM: &str = "macos";

lazy_static::lazy_static! {
    /// 플러그인 정보를 캐시하는 전역 맵
    static ref PLUGIN_INFO: Arc<Mutex<HashMap<String, PluginInfo>>> = Arc::new(Mutex::new(HashMap::new()));
}

/// 플러그인 매니저의 메타정보 (모든 플러그인 목록)
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ManagerMeta {
    /// 플러그인 목록의 버전
    pub version: String,
    /// 플러그인 목록의 설명
    pub description: String,
    /// 사용 가능한 플러그인들
    pub plugins: Vec<PluginMeta>,
}

/// 플러그인 소스 정보 (플러그인을 제공하는 서버)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSource {
    /// 플러그인 소스의 이름
    pub name: String,
    /// 플러그인 소스의 URL
    pub url: String,
    /// 플러그인 소스의 설명
    pub description: String,
}

/// 플러그인의 전체 정보 (메타정보 + 소스 + 설치 상태)
#[derive(Debug, Serialize)]
pub struct PluginInfo {
    /// 플러그인의 출처
    pub source: PluginSource,
    /// 플러그인의 메타정보
    pub meta: PluginMeta,
    /// 설치된 플러그인의 버전 (비어있으면 미설치)
    pub installed_version: String,
    /// 플러그인이 유효하지 않은 이유 (비어있으면 유효)
    pub invalid_reason: String,
}

/// 로컬 플러그인 소스 상수
static PLUGIN_SOURCE_LOCAL: &str = "local";

/// 플러그인 소스 목록을 반환합니다
/// 현재는 하나의 소스만 지원합니다
fn get_plugin_source_list() -> Vec<PluginSource> {
    // 현재는 소스가 하나뿐입니다.
    // vec![PluginSource {
    //     name: "rustdesk".to_string(),
    //     url: "https://raw.githubusercontent.com/fufesou/rustdesk-plugins/main".to_string(),
    //     description: "".to_string(),
    // }]
    vec![]
}

/// 플러그인 소스에서 플러그인 목록을 가져옵니다
fn get_source_plugins() -> HashMap<String, PluginInfo> {
    let mut plugins = HashMap::new();
    for source in get_plugin_source_list().into_iter() {
        // 메타 파일 URL 구성
        let url = format!("{}/meta.toml", source.url);
        match create_http_client().get(&url).send() {
            Ok(resp) => {
                if !resp.status().is_success() {
                    log::error!(
                        "Failed to get plugin list from '{}', status code: {}",
                        url,
                        resp.status()
                    );
                }
                if let Ok(text) = resp.text() {
                    // TOML 파싱
                    match toml::from_str::<ManagerMeta>(&text) {
                        Ok(manager_meta) => {
                            for meta in manager_meta.plugins.iter() {
                                // 현재 플랫폼이 지원되는지 확인
                                if !meta
                                    .platforms
                                    .to_uppercase()
                                    .contains(&PLUGIN_PLATFORM.to_uppercase())
                                {
                                    continue;
                                }
                                plugins.insert(
                                    meta.id.clone(),
                                    PluginInfo {
                                        source: source.clone(),
                                        meta: meta.clone(),
                                        installed_version: "".to_string(),
                                        invalid_reason: "".to_string(),
                                    },
                                );
                            }
                        }
                        Err(e) => log::error!("Failed to parse plugin list from '{}', {}", url, e),
                    }
                }
            }
            Err(e) => log::error!("Failed to get plugin list from '{}', {}", url, e),
        }
    }
    plugins
}

/// 플러그인 목록을 UI로 전송합니다
fn send_plugin_list_event(plugins: &HashMap<String, PluginInfo>) {
    let mut plugin_list = plugins.values().collect::<Vec<_>>();
    // 플러그인 이름 순서로 정렬
    plugin_list.sort_by(|a, b| a.meta.name.cmp(&b.meta.name));
    if let Ok(plugin_list) = serde_json::to_string(&plugin_list) {
        let mut m = HashMap::new();
        m.insert("name", MSG_TO_UI_TYPE_PLUGIN_MANAGER);
        m.insert(MSG_TO_UI_PLUGIN_MANAGER_LIST, &plugin_list);
        if let Ok(event) = serde_json::to_string(&m) {
            let _res = flutter::push_global_event(flutter::APP_TYPE_MAIN, event.clone());
        }
    }
}

/// 플러그인 목록을 로드하고 UI에 전송합니다
pub fn load_plugin_list() {
    let mut plugin_info_lock = PLUGIN_INFO.lock().unwrap();
    // 소스 플러그인 목록 가져오기
    let mut plugins = get_source_plugins();

    // 경합 조건을 방지하기 위해 큰 읽기 잠금이 필요합니다.
    // 플러그인 목록 로딩은 느릴 수 있습니다.
    // 사용자가 이 중에 플러그인을 언로드할 수 있습니다.
    let plugin_infos = super::plugins::get_plugin_infos();
    let plugin_infos_read_lock = plugin_infos.read().unwrap();
    for (id, info) in plugin_infos_read_lock.iter() {
        // 언로드되도록 표시된 플러그인은 스킵
        if info.uninstalled {
            continue;
        }

        if let Some(p) = plugins.get_mut(id) {
            // 소스에서 가져온 플러그인의 설치 버전 업데이트
            p.installed_version = info.desc.meta().version.clone();
            p.invalid_reason = "".to_string();
        } else {
            // 로컬에만 있는 플러그인 추가
            plugins.insert(
                id.to_string(),
                PluginInfo {
                    source: PluginSource {
                        name: PLUGIN_SOURCE_LOCAL.to_string(),
                        url: PLUGIN_SOURCE_LOCAL_DIR.to_string(),
                        description: "".to_string(),
                    },
                    meta: info.desc.meta().clone(),
                    installed_version: info.desc.meta().version.clone(),
                    invalid_reason: "".to_string(),
                },
            );
        }
    }
    // UI에 플러그인 목록 전송
    send_plugin_list_event(&plugins);
    *plugin_info_lock = plugins;
}

/// Windows에서 플러그인 설치를 위해 권한 상승을 수행합니다
#[cfg(target_os = "windows")]
fn elevate_install(
    plugin_id: &str,
    plugin_url: &str,
    same_plugin_exists: bool,
) -> ResultType<bool> {
    // TODO: 인용부호 안의 공백이 있는 인자를 지원합니다. 'arg 1', "arg 2"
    let args = if same_plugin_exists {
        format!("--plugin-install {}", plugin_id)
    } else {
        format!("--plugin-install {} {}", plugin_id, plugin_url)
    };
    crate::platform::elevate(&args)
}

/// Linux에서 플러그인 설치를 위해 권한 상승을 수행합니다
#[cfg(target_os = "linux")]
fn elevate_install(
    plugin_id: &str,
    plugin_url: &str,
    same_plugin_exists: bool,
) -> ResultType<bool> {
    let mut args = vec!["--plugin-install", plugin_id];
    if !same_plugin_exists {
        args.push(&plugin_url);
    }
    crate::platform::elevate(args)
}

/// macOS에서 플러그인 설치를 위해 권한 상승을 수행합니다
#[cfg(target_os = "macos")]
fn elevate_install(
    plugin_id: &str,
    plugin_url: &str,
    same_plugin_exists: bool,
) -> ResultType<bool> {
    let mut args = vec!["--plugin-install", plugin_id];
    if !same_plugin_exists {
        args.push(&plugin_url);
    }
    crate::platform::elevate(args, "RustDesk wants to install then plugin")
}

/// Windows에서 플러그인 언로드를 위해 권한 상승을 수행합니다
#[inline]
#[cfg(target_os = "windows")]
fn elevate_uninstall(plugin_id: &str) -> ResultType<bool> {
    crate::platform::elevate(&format!("--plugin-uninstall {}", plugin_id))
}

/// Linux에서 플러그인 언로드를 위해 권한 상승을 수행합니다
#[inline]
#[cfg(target_os = "linux")]
fn elevate_uninstall(plugin_id: &str) -> ResultType<bool> {
    crate::platform::elevate(vec!["--plugin-uninstall", plugin_id])
}

/// macOS에서 플러그인 언로드를 위해 권한 상승을 수행합니다
#[inline]
#[cfg(target_os = "macos")]
fn elevate_uninstall(plugin_id: &str) -> ResultType<bool> {
    crate::platform::elevate(
        vec!["--plugin-uninstall", plugin_id],
        "RustDesk wants to uninstall the plugin",
    )
}

/// 플러그인을 설치합니다
pub fn install_plugin(id: &str) -> ResultType<()> {
    match PLUGIN_INFO.lock().unwrap().get(id) {
        Some(plugin) => {
            let mut same_plugin_exists = false;
            if let Some(version) = super::plugins::get_version(id) {
                if version == plugin.meta.version {
                    same_plugin_exists = true;
                }
            }
            let plugin_url = format!(
                "{}/plugins/{}/{}/{}_{}.zip",
                plugin.source.url,
                plugin.meta.id,
                PLUGIN_PLATFORM,
                plugin.meta.id,
                plugin.meta.version
            );
            let allowed_install = elevate_install(id, &plugin_url, same_plugin_exists)?;
            if allowed_install && same_plugin_exists {
                super::ipc::load_plugin(id)?;
                super::plugins::load_plugin(id)?;
                super::plugins::mark_uninstalled(id, false);
                push_install_event(id, "finished");
            }
            Ok(())
        }
        None => {
            bail!("Plugin not found: {}", id);
        }
    }
}

/// 언로드되도록 표시된 플러그인 목록을 가져옵니다
fn get_uninstalled_plugins(uninstalled_plugin_set: &HashSet<String>) -> ResultType<Vec<String>> {
    let plugins_dir = super::get_plugins_dir()?;
    let mut plugins = Vec::new();
    if plugins_dir.exists() {
        for entry in std::fs::read_dir(plugins_dir)? {
            match entry {
                Ok(entry) => {
                    let plugin_dir = entry.path();
                    if plugin_dir.is_dir() {
                        if let Some(id) = plugin_dir.file_name().and_then(|n| n.to_str()) {
                            // 언로드 목록에 포함되어 있는지 확인
                            if uninstalled_plugin_set.contains(id) {
                                plugins.push(id.to_string());
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to read plugins dir entry, {}", e);
                }
            }
        }
    }
    Ok(plugins)
}

/// 언로드된 플러그인들을 실제로 제거합니다
pub fn remove_uninstalled() -> ResultType<()> {
    let mut uninstalled_plugin_set = get_uninstall_id_set()?;
    for id in get_uninstalled_plugins(&uninstalled_plugin_set)?.iter() {
        // 플러그인 설정 제거
        super::config::remove(id as _);
        // 플러그인 디렉토리 제거
        if let Ok(dir) = super::get_plugin_dir(id as _) {
            allow_err!(remove_dir_all(dir.clone()));
            // 디렉토리가 실제로 제거되었으면 언로드 목록에서 제거
            if !dir.exists() {
                uninstalled_plugin_set.remove(id);
            }
        }
    }
    allow_err!(update_uninstall_id_set(uninstalled_plugin_set));
    Ok(())
}

/// 플러그인을 언로드합니다
pub fn uninstall_plugin(id: &str, called_by_ui: bool) {
    if called_by_ui {
        match elevate_uninstall(id) {
            Ok(true) => {
                if let Err(e) = super::ipc::uninstall_plugin(id) {
                    log::error!("Failed to uninstall plugin '{}': {}", id, e);
                    push_uninstall_event(id, "failed");
                    return;
                }
                super::plugins::unload_plugin(id);
                super::plugins::mark_uninstalled(id, true);
                super::config::remove(id);
                push_uninstall_event(id, "");
            }
            Ok(false) => {
                return;
            }
            Err(e) => {
                log::error!(
                    "Failed to uninstall plugin '{}', check permission error: {}",
                    id,
                    e
                );
                push_uninstall_event(id, "failed");
                return;
            }
        }
    }

    if super::is_server_running() {
        super::plugins::unload_plugin(&id);
    }
}

/// 플러그인 이벤트를 UI로 푸시합니다
fn push_event(id: &str, r#type: &str, msg: &str) {
    let mut m = HashMap::new();
    m.insert("name", MSG_TO_UI_TYPE_PLUGIN_MANAGER);
    m.insert("id", id);
    m.insert(r#type, msg);
    if let Ok(event) = serde_json::to_string(&m) {
        let _res = flutter::push_global_event(flutter::APP_TYPE_MAIN, event.clone());
    }
}

/// 플러그인 언로드 이벤트를 UI로 푸시합니다
#[inline]
fn push_uninstall_event(id: &str, msg: &str) {
    push_event(id, MSG_TO_UI_PLUGIN_MANAGER_UNINSTALL, msg);
}

/// 플러그인 설치 이벤트를 UI로 푸시합니다
#[inline]
fn push_install_event(id: &str, msg: &str) {
    push_event(id, MSG_TO_UI_PLUGIN_MANAGER_INSTALL, msg);
}

/// IPC 연결을 처리합니다
async fn handle_conn(mut stream: crate::ipc::Connection) {
    loop {
        tokio::select! {
            res = stream.next() => {
                match res {
                    Err(err) => {
                        log::trace!("plugin ipc connection closed: {}", err);
                        break;
                    }
                    Ok(Some(data)) => {
                        match &data {
                            crate::ipc::Data::Plugin(super::ipc::Plugin::InstallStatus((id, status))) => {
                                match status {
                                    InstallStatus::Downloading(n) => {
                                        // 다운로드 진행률 전송
                                        push_install_event(&id, &format!("downloading-{}", n));
                                    },
                                    InstallStatus::Installing => {
                                        // 설치 중 상태 전송
                                        push_install_event(&id, "installing");
                                    }
                                    InstallStatus::Finished => {
                                        // 플러그인 로드 및 UI 업데이트
                                        allow_err!(super::plugins::load_plugin(&id));
                                        allow_err!(super::ipc::load_plugin_async(id).await);
                                        std::thread::spawn(load_plugin_list);
                                        push_install_event(&id, "finished");
                                    }
                                    InstallStatus::FailedCreating => {
                                        // 파일 생성 실패
                                        push_install_event(&id, "failed-creating");
                                    }
                                    InstallStatus::FailedDownloading => {
                                        // 다운로드 실패
                                        push_install_event(&id, "failed-downloading");
                                    }
                                    InstallStatus::FailedInstalling => {
                                        // 설치 실패
                                        push_install_event(&id, "failed-installing");
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {
                    }
                }
            }
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tokio::main]
pub async fn start_ipc() {
    match crate::ipc::new_listener(IPC_PLUGIN_POSTFIX).await {
        Ok(mut incoming) => {
            while let Some(result) = incoming.next().await {
                match result {
                    Ok(stream) => {
                        log::debug!("Got new connection");
                        tokio::spawn(handle_conn(crate::ipc::Connection::new(stream)));
                    }
                    Err(err) => {
                        log::error!("Couldn't get plugin client: {:?}", err);
                    }
                }
            }
        }
        Err(err) => {
            log::error!("Failed to start plugin ipc server: {}", err);
        }
    }
}

/// 언로드 목록 파일에서 언로드된 플러그인 ID 세트를 가져옵니다
pub(super) fn get_uninstall_id_set() -> ResultType<HashSet<String>> {
    let uninstall_file_path = super::get_uninstall_file_path()?;
    if !uninstall_file_path.exists() {
        std::fs::create_dir_all(&super::get_plugins_dir()?)?;
        return Ok(HashSet::new());
    }
    let s = read_to_string(uninstall_file_path)?;
    Ok(serde_json::from_str::<HashSet<String>>(&s)?)
}

/// 언로드 목록을 파일에 저장합니다
fn update_uninstall_id_set(set: HashSet<String>) -> ResultType<()> {
    let content = serde_json::to_string(&set)?;
    let file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(super::get_uninstall_file_path()?)?;
    let mut writer = std::io::BufWriter::new(file);
    writer.write_all(content.as_bytes())?;
    Ok(())
}

// 플러그인 설치 프로세스 관련 모듈
pub(super) mod install {
    use super::IPC_PLUGIN_POSTFIX;
    use crate::hbbs_http::create_http_client;
    use crate::{
        ipc::{connect, Data},
        plugin::ipc::{InstallStatus, Plugin},
    };
    use hbb_common::{allow_err, bail, log, tokio, ResultType};
    use std::{
        fs::File,
        io::{BufReader, BufWriter, Write},
        path::Path,
    };
    use zip::ZipArchive;

    /// 설치 상태를 IPC로 전송합니다
    #[tokio::main(flavor = "current_thread")]
    async fn send_install_status(id: &str, status: InstallStatus) {
        allow_err!(_send_install_status(id, status).await);
    }

    /// 실제로 설치 상태를 IPC로 전송합니다
    async fn _send_install_status(id: &str, status: InstallStatus) -> ResultType<()> {
        let mut c = connect(1_000, IPC_PLUGIN_POSTFIX).await?;
        c.send(&Data::Plugin(Plugin::InstallStatus((
            id.to_string(),
            status,
        ))))
        .await?;
        Ok(())
    }

    /// HTTP에서 파일로 다운로드합니다
    fn download_to_file(url: &str, file: File) -> ResultType<()> {
        let resp = match create_http_client().get(url).send() {
            Ok(resp) => resp,
            Err(e) => {
                bail!("get plugin from '{}', {}", url, e);
            }
        };

        if !resp.status().is_success() {
            bail!("get plugin from '{}', status code: {}", url, resp.status());
        }

        let mut writer = BufWriter::new(file);
        writer.write_all(resp.bytes()?.as_ref())?;
        Ok(())
    }

    /// 플러그인 파일을 다운로드합니다
    fn download_file(id: &str, url: &str, filename: &Path) -> bool {
        let file = match File::create(filename) {
            Ok(f) => f,
            Err(e) => {
                log::error!("Failed to create plugin file: {}", e);
                send_install_status(id, InstallStatus::FailedCreating);
                return false;
            }
        };
        if let Err(e) = download_to_file(url, file) {
            log::error!("Failed to download plugin '{}', {}", id, e);
            send_install_status(id, InstallStatus::FailedDownloading);
            return false;
        }
        true
    }

    /// ZIP 파일에서 플러그인을 추출합니다
    fn do_install_file(filename: &Path, target_dir: &Path) -> ResultType<()> {
        let mut zip = ZipArchive::new(BufReader::new(File::open(filename)?))?;
        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            let file_path = target_dir.join(file.name());
            if file.name().ends_with("/") {
                // 디렉토리 생성
                std::fs::create_dir_all(&file_path)?;
            } else {
                // 파일 추출
                if let Some(p) = file_path.parent() {
                    if !p.exists() {
                        std::fs::create_dir_all(&p)?;
                    }
                }
                let mut outfile = File::create(&file_path)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }
        Ok(())
    }

    /// 플러그인을 언로드 목록에서 추가 또는 제거합니다
    pub fn change_uninstall_plugin(id: &str, add: bool) {
        match super::get_uninstall_id_set() {
            Ok(mut set) => {
                if add {
                    set.insert(id.to_string());
                } else {
                    set.remove(id);
                }
                if let Err(e) = super::update_uninstall_id_set(set) {
                    log::error!("Failed to write uninstall list, {}", e);
                }
            }
            Err(e) => log::error!(
                "Failed to get plugins dir, unable to read uninstall list, {}",
                e
            ),
        }
    }

    /// URL에서 플러그인을 다운로드하고 설치합니다
    pub fn install_plugin_with_url(id: &str, url: &str) {
        log::info!("Installing plugin '{}', url: {}", id, url);
        let plugin_dir = match super::super::get_plugin_dir(id) {
            Ok(d) => d,
            Err(e) => {
                send_install_status(id, InstallStatus::FailedCreating);
                log::error!("Failed to get plugin dir: {}", e);
                return;
            }
        };
        if !plugin_dir.exists() {
            if let Err(e) = std::fs::create_dir_all(&plugin_dir) {
                send_install_status(id, InstallStatus::FailedCreating);
                log::error!("Failed to create plugin dir: {}", e);
                return;
            }
        }

        let filename = match url.rsplit('/').next() {
            Some(filename) => plugin_dir.join(filename),
            None => {
                send_install_status(id, InstallStatus::FailedDownloading);
                log::error!("Failed to download plugin file, invalid url: {}", url);
                return;
            }
        };

        let filename_to_remove = filename.clone();
        let _call_on_ret = crate::common::SimpleCallOnReturn {
            b: true,
            f: Box::new(move || {
                if let Err(e) = std::fs::remove_file(&filename_to_remove) {
                    log::error!("Failed to remove plugin file: {}", e);
                }
            }),
        };

        // download
        if !download_file(id, url, &filename) {
            return;
        }

        // install
        send_install_status(id, InstallStatus::Installing);
        if let Err(e) = do_install_file(&filename, &plugin_dir) {
            log::error!("Failed to install plugin: {}", e);
            send_install_status(id, InstallStatus::FailedInstalling);
            return;
        }

        // finished
        send_install_status(id, InstallStatus::Finished);
    }
}
