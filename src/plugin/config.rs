use super::{cstr_to_string, str_to_cstr_ret};
use hbb_common::{allow_err, bail, config::Config as HbbConfig, lazy_static, log, ResultType};
use serde_derive::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ffi::c_char,
    fs,
    ops::{Deref, DerefMut},
    path::PathBuf,
    ptr,
    str::FromStr,
    sync::{Arc, Mutex},
};

lazy_static::lazy_static! {
    /// 플러그인의 공유 설정을 저장하는 맵
    static ref CONFIG_SHARED: Arc<Mutex<HashMap<String, SharedConfig>>> = Default::default();
    /// 플러그인의 피어별 설정을 저장하는 맵
    static ref CONFIG_PEERS: Arc<Mutex<HashMap<String, PeersConfig>>> = Default::default();
    /// 플러그인 매니저 설정을 저장
    static ref CONFIG_MANAGER: Arc<Mutex<ManagerConfig>> = {
        let conf = hbb_common::config::load_path::<ManagerConfig>(ManagerConfig::path());
        Arc::new(Mutex::new(conf))
    };
}
use crate::ui_interface::get_id;

/// 공유 설정 타입 상수
pub(super) const CONFIG_TYPE_SHARED: &str = "shared";
/// 피어별 설정 타입 상수
pub(super) const CONFIG_TYPE_PEER: &str = "peer";

/// 플러그인의 공유 설정을 저장하는 구조체
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SharedConfig(HashMap<String, String>);

/// 플러그인의 피어별 설정을 저장하는 구조체
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PeerConfig(HashMap<String, String>);

/// 플러그인의 모든 피어 설정을 저장하는 타입 (플러그인 ID -> 피어 설정)
type PeersConfig = HashMap<String, PeerConfig>;

/// 플러그인 디렉토리의 경로를 반환합니다
#[inline]
fn path_plugins(id: &str) -> PathBuf {
    HbbConfig::path("plugins").join(id)
}

/// 플러그인을 제거합니다 (설정 및 파일 삭제)
pub fn remove(id: &str) {
    // 공유 설정 제거
    CONFIG_SHARED.lock().unwrap().remove(id);
    // 피어별 설정 제거
    CONFIG_PEERS.lock().unwrap().remove(id);
    // 매니저 설정에서도 플러그인 제거 (에러는 무시)
    allow_err!(ManagerConfig::remove_plugin(id));
    // 플러그인 디렉토리 제거
    if let Err(e) = fs::remove_dir_all(path_plugins(id)) {
        log::error!("Failed to remove plugin '{}' directory: {}", id, e);
    }
}

impl Deref for SharedConfig {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SharedConfig {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Deref for PeerConfig {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for PeerConfig {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl SharedConfig {
    /// 플러그인의 공유 설정 파일 경로를 반환합니다
    #[inline]
    fn path(id: &str) -> PathBuf {
        path_plugins(id).join("shared.toml")
    }

    /// 플러그인의 공유 설정을 디스크에서 메모리로 로드합니다
    /// 플러그인 설명의 기본값으로 누락된 설정을 채웁니다
    #[inline]
    fn load(id: &str) {
        let mut lock = CONFIG_SHARED.lock().unwrap();
        if lock.contains_key(id) {
            return;
        }
        // TOML 파일에서 설정 로드
        let conf = hbb_common::config::load_path::<HashMap<String, String>>(Self::path(id));
        let mut conf = SharedConfig(conf);
        // 플러그인 설명에 정의된 기본값으로 누락된 항목을 채웁니다
        if let Some(desc_conf) = super::plugins::get_desc_conf(id) {
            for item in desc_conf.shared.iter() {
                if !conf.contains_key(&item.key) {
                    conf.insert(item.key.to_owned(), item.default.to_owned());
                }
            }
        }
        lock.insert(id.to_owned(), conf);
    }

    /// 플러그인의 공유 설정이 아직 로드되지 않았다면 로드합니다
    #[inline]
    fn load_if_not_exists(id: &str) {
        if CONFIG_SHARED.lock().unwrap().contains_key(id) {
            return;
        }
        Self::load(id);
    }

    /// 플러그인의 공유 설정에서 값을 가져옵니다
    #[inline]
    pub fn get(id: &str, key: &str) -> Option<String> {
        Self::load_if_not_exists(id);
        CONFIG_SHARED
            .lock()
            .unwrap()
            .get(id)?
            .get(key)
            .map(|s| s.to_owned())
    }

    /// 플러그인의 공유 설정에 값을 저장합니다
    #[inline]
    pub fn set(id: &str, key: &str, value: &str) -> ResultType<()> {
        Self::load_if_not_exists(id);
        match CONFIG_SHARED.lock().unwrap().get_mut(id) {
            Some(config) => {
                // 메모리에 저장
                config.insert(key.to_owned(), value.to_owned());
                // 디스크에도 저장
                hbb_common::config::store_path(Self::path(id), config)
            }
            None => {
                // 도달할 수 없는 경우
                bail!("No such plugin {}", id)
            }
        }
    }
}

impl PeerConfig {
    /// 플러그인의 피어별 설정 파일 경로를 반환합니다
    #[inline]
    fn path(id: &str, peer: &str) -> PathBuf {
        path_plugins(id)
            .join("peers")
            .join(format!("{}.toml", peer))
    }

    /// 특정 피어의 설정을 디스크에서 메모리로 로드합니다
    /// 플러그인 설명의 기본값으로 누락된 설정을 채웁니다
    #[inline]
    fn load(id: &str, peer: &str) {
        let mut lock = CONFIG_PEERS.lock().unwrap();
        // 이미 로드되었는지 확인
        if let Some(peers) = lock.get(id) {
            if peers.contains_key(peer) {
                return;
            }
        }

        // TOML 파일에서 설정 로드
        let conf = hbb_common::config::load_path::<HashMap<String, String>>(Self::path(id, peer));
        let mut conf = PeerConfig(conf);
        // 플러그인 설명에 정의된 기본값으로 누락된 항목을 채웁니다
        if let Some(desc_conf) = super::plugins::get_desc_conf(id) {
            for item in desc_conf.peer.iter() {
                if !conf.contains_key(&item.key) {
                    conf.insert(item.key.to_owned(), item.default.to_owned());
                }
            }
        }

        // 플러그인이 이미 존재하면 피어 설정 추가
        if let Some(peers) = lock.get_mut(id) {
            peers.insert(peer.to_owned(), conf);
            return;
        }

        // 플러그인이 존재하지 않으면 새로 생성
        let mut peers = HashMap::new();
        peers.insert(peer.to_owned(), conf);
        lock.insert(id.to_owned(), peers);
    }

    /// 특정 피어의 설정이 아직 로드되지 않았다면 로드합니다
    #[inline]
    fn load_if_not_exists(id: &str, peer: &str) {
        if let Some(peers) = CONFIG_PEERS.lock().unwrap().get(id) {
            if peers.contains_key(peer) {
                return;
            }
        }
        Self::load(id, peer);
    }

    /// 특정 피어의 설정에서 값을 가져옵니다
    #[inline]
    pub fn get(id: &str, peer: &str, key: &str) -> Option<String> {
        Self::load_if_not_exists(id, peer);
        CONFIG_PEERS
            .lock()
            .unwrap()
            .get(id)?
            .get(peer)?
            .get(key)
            .map(|s| s.to_owned())
    }

    /// 특정 피어의 설정에 값을 저장합니다
    #[inline]
    pub fn set(id: &str, peer: &str, key: &str, value: &str) -> ResultType<()> {
        Self::load_if_not_exists(id, peer);
        match CONFIG_PEERS.lock().unwrap().get_mut(id) {
            Some(peers) => match peers.get_mut(peer) {
                Some(config) => {
                    // 메모리에 저장
                    config.insert(key.to_owned(), value.to_owned());
                    // 디스크에도 저장
                    hbb_common::config::store_path(Self::path(id, peer), config)
                }
                None => {
                    // 도달할 수 없는 경우
                    bail!("No such peer {}", peer)
                }
            },
            None => {
                // 도달할 수 없는 경우
                bail!("No such plugin {}", id)
            }
        }
    }
}

/// 플러그인의 활성화 상태를 저장하는 구조체
#[derive(Debug, Serialize, Deserialize)]
pub struct PluginStatus {
    /// 플러그인 활성화 여부
    pub enabled: bool,
}

/// 플러그인 매니저 설정 파일의 버전
const MANAGER_VERSION: &str = "0.1.0";

/// 플러그인 매니저의 전역 설정을 저장하는 구조체
#[derive(Debug, Serialize, Deserialize)]
pub struct ManagerConfig {
    /// 설정 파일의 버전
    pub version: String,
    /// 매니저의 옵션 (키-값 쌍)
    #[serde(default)]
    pub options: HashMap<String, String>,
    /// 플러그인별 상태 (플러그인 ID -> 활성화 상태)
    #[serde(default)]
    pub plugins: HashMap<String, PluginStatus>,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            version: MANAGER_VERSION.to_owned(),
            options: HashMap::new(),
            plugins: HashMap::new(),
        }
    }
}

// store_path의 에러는 무시합니다. 실패 시 이전 값을 복원할 필요가 없습니다.
impl ManagerConfig {
    /// 플러그인 매니저 설정 파일의 경로를 반환합니다
    #[inline]
    fn path() -> PathBuf {
        HbbConfig::path("plugins").join("manager.toml")
    }

    /// 매니저 옵션을 가져옵니다
    #[inline]
    pub fn get_option(key: &str) -> Option<String> {
        CONFIG_MANAGER
            .lock()
            .unwrap()
            .options
            .get(key)
            .map(|s| s.to_owned())
    }

    /// 매니저 옵션을 설정합니다
    #[inline]
    pub fn set_option(key: &str, value: &str) {
        let mut lock = CONFIG_MANAGER.lock().unwrap();
        lock.options.insert(key.to_owned(), value.to_owned());
        allow_err!(hbb_common::config::store_path(Self::path(), &*lock));
    }

    /// 플러그인의 옵션을 가져옵니다 (현재는 "enabled" 옵션만 지원)
    #[inline]
    pub fn get_plugin_option(id: &str, key: &str) -> Option<String> {
        let lock = CONFIG_MANAGER.lock().unwrap();
        match key {
            "enabled" => {
                // 플러그인이 명시적으로 비활성화되지 않으면 활성화 상태로 간주
                let enabled = lock
                    .plugins
                    .get(id)
                    .map(|status| status.enabled.to_owned())
                    .unwrap_or(true.to_owned())
                    .to_string();
                Some(enabled)
            }
            _ => None,
        }
    }

    /// 플러그인의 활성화 상태를 설정합니다
    fn set_plugin_option_enabled(id: &str, enabled: bool) -> ResultType<()> {
        let mut lock = CONFIG_MANAGER.lock().unwrap();
        if let Some(status) = lock.plugins.get_mut(id) {
            status.enabled = enabled;
        } else {
            lock.plugins.insert(id.to_owned(), PluginStatus { enabled });
        }
        hbb_common::config::store_path(Self::path(), &*lock)
    }

    /// 플러그인의 옵션을 설정합니다
    /// 활성화 상태 변경 시 플러그인을 자동으로 로드/언로드합니다
    pub fn set_plugin_option(id: &str, key: &str, value: &str) {
        match key {
            "enabled" => {
                let enabled = bool::from_str(value).unwrap_or(false);
                allow_err!(Self::set_plugin_option_enabled(id, enabled));
                if enabled {
                    // 활성화: 플러그인 로드
                    allow_err!(super::load_plugin(id));
                } else {
                    // 비활성화: 플러그인 언로드
                    super::unload_plugin(id);
                }
            }
            _ => log::error!("No such option {}", key),
        }
    }

    /// 플러그인을 매니저 설정에 추가합니다 (활성화 상태로 추가)
    #[inline]
    pub fn add_plugin(id: &str) -> ResultType<()> {
        let mut lock = CONFIG_MANAGER.lock().unwrap();
        lock.plugins
            .insert(id.to_owned(), PluginStatus { enabled: true });
        hbb_common::config::store_path(Self::path(), &*lock)
    }

    /// 플러그인을 매니저 설정에서 제거합니다
    #[inline]
    pub fn remove_plugin(id: &str) -> ResultType<()> {
        let mut lock = CONFIG_MANAGER.lock().unwrap();
        lock.plugins.remove(id);
        hbb_common::config::store_path(Self::path(), &*lock)
    }
}

/// 로컬 피어 ID를 반환하는 콜백 함수
pub(super) extern "C" fn cb_get_local_peer_id() -> *const c_char {
    str_to_cstr_ret(&get_id())
}

/// 설정 값을 가져오는 콜백 함수
/// peer가 nullptr이면 공유 설정을 반환합니다
pub(super) extern "C" fn cb_get_conf(
    peer: *const c_char,
    id: *const c_char,
    key: *const c_char,
) -> *const c_char {
    match (cstr_to_string(id), cstr_to_string(key)) {
        (Ok(id), Ok(key)) => {
            if peer.is_null() {
                SharedConfig::load_if_not_exists(&id);
                if let Some(conf) = CONFIG_SHARED.lock().unwrap().get(&id) {
                    if let Some(value) = conf.get(&key) {
                        return str_to_cstr_ret(value);
                    }
                }
            } else {
                match cstr_to_string(peer) {
                    Ok(peer) => {
                        PeerConfig::load_if_not_exists(&id, &peer);
                        if let Some(conf) = CONFIG_PEERS.lock().unwrap().get(&id) {
                            if let Some(conf) = conf.get(&peer) {
                                if let Some(value) = conf.get(&key) {
                                    return str_to_cstr_ret(value);
                                }
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
        }
        _ => {}
    }
    ptr::null()
}
