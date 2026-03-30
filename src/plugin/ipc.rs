// TODO: 이 모듈과 crate::ipc 간의 상호 의존성이 좋은 설계가 아닙니다.
use crate::ipc::{connect, Connection, Data};
use hbb_common::{allow_err, log, tokio, ResultType};
use serde_derive::{Deserialize, Serialize};

/// 플러그인 설치 상태를 나타내는 열거형
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum InstallStatus {
    /// 다운로드 중 (백분율)
    Downloading(u8),
    /// 설치 중
    Installing,
    /// 설치 완료
    Finished,
    /// 플러그인 파일 생성 실패
    FailedCreating,
    /// 플러그인 다운로드 실패
    FailedDownloading,
    /// 플러그인 설치 실패
    FailedInstalling,
}

/// IPC를 통해 처리할 플러그인 명령을 나타내는 열거형
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "t", content = "c")]
pub enum Plugin {
    /// 플러그인 설정 (플러그인 ID, 키, 값)
    Config(String, String, Option<String>),
    /// 매니저 설정 (키, 값)
    ManagerConfig(String, Option<String>),
    /// 매니저 플러그인 설정 (플러그인 ID, 키, 값)
    ManagerPluginConfig(String, String, Option<String>),
    /// 플러그인 로드 (플러그인 ID)
    Load(String),
    /// 플러그인 다시 로드 (플러그인 ID)
    Reload(String),
    /// 플러그인 설치 상태 (플러그인 ID, 상태)
    InstallStatus((String, InstallStatus)),
    /// 플러그인 언로드 (플러그인 ID)
    Uninstall(String),
}

/// 플러그인 설정 값을 가져옵니다
#[tokio::main(flavor = "current_thread")]
pub async fn get_config(id: &str, name: &str) -> ResultType<Option<String>> {
    get_config_async(id, name, 1_000).await
}

/// 플러그인 설정 값을 설정합니다
#[tokio::main(flavor = "current_thread")]
pub async fn set_config(id: &str, name: &str, value: String) -> ResultType<()> {
    set_config_async(id, name, value).await
}

/// 매니저 설정 값을 가져옵니다
#[tokio::main(flavor = "current_thread")]
pub async fn get_manager_config(name: &str) -> ResultType<Option<String>> {
    get_manager_config_async(name, 1_000).await
}

/// 매니저 설정 값을 설정합니다
#[tokio::main(flavor = "current_thread")]
pub async fn set_manager_config(name: &str, value: String) -> ResultType<()> {
    set_manager_config_async(name, value).await
}

/// 매니저 플러그인 설정 값을 가져옵니다
#[tokio::main(flavor = "current_thread")]
pub async fn get_manager_plugin_config(id: &str, name: &str) -> ResultType<Option<String>> {
    get_manager_plugin_config_async(id, name, 1_000).await
}

/// 매니저 플러그인 설정 값을 설정합니다
#[tokio::main(flavor = "current_thread")]
pub async fn set_manager_plugin_config(id: &str, name: &str, value: String) -> ResultType<()> {
    set_manager_plugin_config_async(id, name, value).await
}

/// 플러그인을 로드합니다
#[tokio::main(flavor = "current_thread")]
pub async fn load_plugin(id: &str) -> ResultType<()> {
    load_plugin_async(id).await
}

/// 플러그인을 다시 로드합니다
#[tokio::main(flavor = "current_thread")]
pub async fn reload_plugin(id: &str) -> ResultType<()> {
    reload_plugin_async(id).await
}

/// 플러그인을 언로드합니다
#[tokio::main(flavor = "current_thread")]
pub async fn uninstall_plugin(id: &str) -> ResultType<()> {
    uninstall_plugin_async(id).await
}

async fn get_config_async(id: &str, name: &str, ms_timeout: u64) -> ResultType<Option<String>> {
    let mut c = connect(ms_timeout, "").await?;
    c.send(&Data::Plugin(Plugin::Config(
        id.to_owned(),
        name.to_owned(),
        None,
    )))
    .await?;
    if let Some(Data::Plugin(Plugin::Config(id2, name2, value))) =
        c.next_timeout(ms_timeout).await?
    {
        if id == id2 && name == name2 {
            return Ok(value);
        }
    }
    return Ok(None);
}

async fn set_config_async(id: &str, name: &str, value: String) -> ResultType<()> {
    let mut c = connect(1000, "").await?;
    c.send(&Data::Plugin(Plugin::Config(
        id.to_owned(),
        name.to_owned(),
        Some(value),
    )))
    .await?;
    Ok(())
}

async fn get_manager_config_async(name: &str, ms_timeout: u64) -> ResultType<Option<String>> {
    let mut c = connect(ms_timeout, "").await?;
    c.send(&Data::Plugin(Plugin::ManagerConfig(name.to_owned(), None)))
        .await?;
    if let Some(Data::Plugin(Plugin::ManagerConfig(name2, value))) =
        c.next_timeout(ms_timeout).await?
    {
        if name == name2 {
            return Ok(value);
        }
    }
    return Ok(None);
}

async fn set_manager_config_async(name: &str, value: String) -> ResultType<()> {
    let mut c = connect(1000, "").await?;
    c.send(&Data::Plugin(Plugin::ManagerConfig(
        name.to_owned(),
        Some(value),
    )))
    .await?;
    Ok(())
}

async fn get_manager_plugin_config_async(
    id: &str,
    name: &str,
    ms_timeout: u64,
) -> ResultType<Option<String>> {
    let mut c = connect(ms_timeout, "").await?;
    c.send(&Data::Plugin(Plugin::ManagerPluginConfig(
        id.to_owned(),
        name.to_owned(),
        None,
    )))
    .await?;
    if let Some(Data::Plugin(Plugin::ManagerPluginConfig(id2, name2, value))) =
        c.next_timeout(ms_timeout).await?
    {
        if id == id2 && name == name2 {
            return Ok(value);
        }
    }
    return Ok(None);
}

async fn set_manager_plugin_config_async(id: &str, name: &str, value: String) -> ResultType<()> {
    let mut c = connect(1000, "").await?;
    c.send(&Data::Plugin(Plugin::ManagerPluginConfig(
        id.to_owned(),
        name.to_owned(),
        Some(value),
    )))
    .await?;
    Ok(())
}

pub async fn load_plugin_async(id: &str) -> ResultType<()> {
    let mut c = connect(1000, "").await?;
    c.send(&Data::Plugin(Plugin::Load(id.to_owned()))).await?;
    Ok(())
}

async fn reload_plugin_async(id: &str) -> ResultType<()> {
    let mut c = connect(1000, "").await?;
    c.send(&Data::Plugin(Plugin::Reload(id.to_owned()))).await?;
    Ok(())
}

async fn uninstall_plugin_async(id: &str) -> ResultType<()> {
    let mut c = connect(1000, "").await?;
    c.send(&Data::Plugin(Plugin::Uninstall(id.to_owned())))
        .await?;
    Ok(())
}

/// IPC로 받은 플러그인 명령을 처리합니다
pub async fn handle_plugin(plugin: Plugin, stream: &mut Connection) {
    match plugin {
        Plugin::Config(id, name, value) => match value {
            None => {
                // 설정 값 가져오기
                let value = super::SharedConfig::get(&id, &name);
                allow_err!(
                    stream
                        .send(&Data::Plugin(Plugin::Config(id, name, value)))
                        .await
                );
            }
            Some(value) => {
                // 설정 값 저장
                allow_err!(super::SharedConfig::set(&id, &name, &value));
            }
        },
        Plugin::ManagerConfig(name, value) => match value {
            None => {
                // 매니저 설정 값 가져오기
                let value = super::ManagerConfig::get_option(&name);
                allow_err!(
                    stream
                        .send(&Data::Plugin(Plugin::ManagerConfig(name, value)))
                        .await
                );
            }
            Some(value) => {
                // 매니저 설정 값 저장
                super::ManagerConfig::set_option(&name, &value);
            }
        },
        Plugin::ManagerPluginConfig(id, name, value) => match value {
            None => {
                // 매니저 플러그인 설정 값 가져오기
                let value = super::ManagerConfig::get_plugin_option(&id, &name);
                allow_err!(
                    stream
                        .send(&Data::Plugin(Plugin::ManagerPluginConfig(id, name, value)))
                        .await
                );
            }
            Some(value) => {
                // 매니저 플러그인 설정 값 저장
                super::ManagerConfig::set_plugin_option(&id, &name, &value);
            }
        },
        Plugin::Load(id) => {
            // 플러그인 로드
            allow_err!(super::load_plugin(&id));
        }
        Plugin::Reload(id) => {
            // 플러그인 다시 로드
            allow_err!(super::reload_plugin(&id));
        }
        Plugin::Uninstall(id) => {
            // 플러그인 언로드
            super::manager::uninstall_plugin(&id, false);
        }
        _ => {}
    }
}
