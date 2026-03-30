use super::create_http_client_async_with_url;
use hbb_common::{
    bail,
    lazy_static::lazy_static,
    log,
    tokio::{
        self,
        fs::File,
        io::AsyncWriteExt,
        sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    },
    ResultType,
};
use serde_derive::Serialize;
use std::{collections::HashMap, path::PathBuf, sync::Mutex, time::Duration};

// 활성 다운로더들을 URL ID로 저장하는 전역 맵
lazy_static! {
    static ref DOWNLOADERS: Mutex<HashMap<String, Downloader>> = Default::default();
}

/// 다운로드 상태와 결과를 반환하는 구조체
/// 호출자는 다운로드 성공 여부를 확인하고 맵에서 작업을 제거해야 함.
/// 다운로드 실패 시: `data` 필드는 비어있음.
/// 다운로드 성공 시: `path`가 None이면 `data` 필드에 다운로드된 데이터 포함.
#[derive(Serialize, Debug)]
pub struct DownloadData {
    // 메모리에 로드된 파일 데이터 (path가 None일 때만 사용)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<u8>,
    // 저장된 파일 경로 (선택사항)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    // 전체 파일 크기 (바이트, 선택사항)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_size: Option<u64>,
    // 지금까지 다운로드된 크기 (바이트)
    pub downloaded_size: u64,
    // 다운로드 오류 메시지 (성공 시 None)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 내부 다운로더 상태를 추적하는 구조체
struct Downloader {
    // 메모리에 버퍼링된 파일 데이터
    data: Vec<u8>,
    // 저장할 파일 경로
    path: Option<PathBuf>,
    // 전체 파일 크기 (일부 파일은 크기가 비어있을 수 있으므로 Option 사용)
    total_size: Option<u64>,
    // 현재까지 다운로드된 크기
    downloaded_size: u64,
    // 다운로드 중 발생한 오류 메시지
    error: Option<String>,
    // 다운로드 완료 여부
    finished: bool,
    // 다운로드 취소 신호를 보낼 채널
    tx_cancel: UnboundedSender<()>,
}

/// 파일 다운로드를 시작하고 고유 ID 반환
/// 호출자는 다운로드 성공 여부를 확인하고 `get_download_data()` 후 ID를 제거해야 함.
pub fn download_file(
    url: String,
    path: Option<PathBuf>,
    auto_del_dur: Option<Duration>,
) -> ResultType<String> {
    let id = url.clone();

    // 1단계: 기존 다운로더 확인
    // - 성공 중인 다운로더가 있으면 재사용
    // - 실패한 다운로더가 있으면 제거하여 재시도 가능
    let mut stale_path = None;
    {
        let mut downloaders = DOWNLOADERS.lock().unwrap();
        if let Some(downloader) = downloaders.get(&id) {
            if downloader.error.is_none() {
                return Ok(id);
            }
            stale_path = downloader.path.clone();
            downloaders.remove(&id);
        }
    }
    // 이전 실패한 파일 정리
    if let Some(p) = stale_path {
        if p.exists() {
            if let Err(e) = std::fs::remove_file(&p) {
                log::warn!("이전 다운로드 파일 제거 실패 {}: {}", p.display(), e);
            }
        }
    }

    // 파일 경로 검증
    if let Some(path) = path.as_ref() {
        if path.exists() {
            bail!("파일이 이미 존재함: {}", path.display());
        }
        // 필요하면 상위 디렉토리 생성
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    // 다운로더 생성
    let (tx, rx) = unbounded_channel();
    let downloader = Downloader {
        data: Vec::new(),
        path: path.clone(),
        total_size: None,
        downloaded_size: 0,
        error: None,
        tx_cancel: tx,
        finished: false,
    };

    // 2단계: 동시 호출 때문에 race condition 방지하기 위해 다시 확인
    let mut stale_path_after_check = None;
    {
        let mut downloaders = DOWNLOADERS.lock().unwrap();
        if let Some(existing) = downloaders.get(&id) {
            if existing.error.is_none() {
                return Ok(id);
            }
            stale_path_after_check = existing.path.clone();
            downloaders.remove(&id);
        }
        downloaders.insert(id.clone(), downloader);
    }
    // 재확인 후 이전 파일 정리
    if let Some(p) = stale_path_after_check {
        if p.exists() {
            if let Err(e) = std::fs::remove_file(&p) {
                log::warn!("이전 다운로드 파일 제거 실패 {}: {}", p.display(), e);
            }
        }
    }

    // 백그라운드 스레드에서 실제 다운로드 수행
    let id2 = id.clone();
    std::thread::spawn(
        move || match do_download(&id2, url, path, auto_del_dur, rx) {
            Ok(is_all_downloaded) => {
                let mut downloaded_size = 0;
                let mut total_size = 0;
                DOWNLOADERS.lock().unwrap().get_mut(&id2).map(|downloader| {
                    downloaded_size = downloader.downloaded_size;
                    total_size = downloader.total_size.unwrap_or(0);
                });
                log::info!(
                    "다운로드 {} 완료, {}/{}, {:.2} %",
                    &id2,
                    downloaded_size,
                    total_size,
                    if total_size == 0 {
                        0.0
                    } else {
                        downloaded_size as f64 / total_size as f64 * 100.0
                    }
                );

                // 사용자가 취소한 경우 부분 다운로드 파일 삭제
                let is_canceled = !is_all_downloaded;
                if is_canceled {
                    if let Some(downloader) = DOWNLOADERS.lock().unwrap().remove(&id2) {
                        if let Some(p) = downloader.path {
                            if p.exists() {
                                std::fs::remove_file(p).ok();
                            }
                        }
                    }
                }
            }
            Err(e) => {
                let err = e.to_string();
                log::error!("다운로드 {} 실패: {}", &id2, &err);
                DOWNLOADERS.lock().unwrap().get_mut(&id2).map(|downloader| {
                    downloader.error = Some(err);
                });
            }
        },
    );

    Ok(id)
}

/// 실제 다운로드를 수행하는 비동기 함수
/// 파일 크기 확인 -> 전체 파일 다운로드 -> 디스크에 저장 (또는 메모리에 버퍼링)
/// 취소 신호 수신 시 중단 가능
#[tokio::main(flavor = "current_thread")]
async fn do_download(
    id: &str,
    url: String,
    path: Option<PathBuf>,
    auto_del_dur: Option<Duration>,
    mut rx_cancel: UnboundedReceiver<()>,
) -> ResultType<bool> {
    let client = create_http_client_async_with_url(&url).await;

    let mut is_all_downloaded = false;

    // 단계 1: HEAD 요청으로 파일 크기 확인
    tokio::select! {
        _ = rx_cancel.recv() => {
            return Ok(is_all_downloaded);
        }
        head_resp = client.head(&url).send() => {
            match head_resp {
                Ok(resp) => {
                    if resp.status().is_success() {
                        let total_size = resp
                            .headers()
                            .get(reqwest::header::CONTENT_LENGTH)
                            .and_then(|ct_len| ct_len.to_str().ok())
                            .and_then(|ct_len| ct_len.parse::<u64>().ok());
                        let Some(total_size) = total_size else {
                            bail!("컨텐츠 길이 획득 실패");
                        };
                        DOWNLOADERS.lock().unwrap().get_mut(id).map(|downloader| {
                            downloader.total_size = Some(total_size);
                        });
                    } else {
                        bail!("컨텐츠 길이 획득 실패: {}", resp.status());
                    }
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
    }

    // 단계 2: GET 요청으로 파일 다운로드 시작
    let mut response;
    tokio::select! {
        _ = rx_cancel.recv() => {
            return Ok(is_all_downloaded);
        }
        resp = client.get(url).send() => {
            response = resp?;
        }
    }

    // 단계 3: 저장 대상 준비 (파일 또는 메모리 버퍼)
    let mut dest: Option<File> = None;
    if let Some(p) = path {
        dest = Some(File::create(p).await?);
    }

    // 단계 4: 청크 단위로 다운로드하여 저장 또는 버퍼링
    loop {
        tokio::select! {
            _ = rx_cancel.recv() => {
                // 취소 신호 수신 - 루프 종료
                break;
            }
            chunk = response.chunk() => {
                match chunk {
                    Ok(Some(chunk)) => {
                        match dest {
                            Some(ref mut f) => {
                                // 파일에 저장
                                f.write_all(&chunk).await?;
                                f.flush().await?;
                                DOWNLOADERS.lock().unwrap().get_mut(id).map(|downloader| {
                                    downloader.downloaded_size += chunk.len() as u64;
                                });
                            }
                            None => {
                                // 메모리에 버퍼링
                                DOWNLOADERS.lock().unwrap().get_mut(id).map(|downloader| {
                                    downloader.data.extend_from_slice(&chunk);
                                    downloader.downloaded_size += chunk.len() as u64;
                                });
                            }
                        }
                    }
                    Ok(None) => {
                        // 모든 청크 수신 완료
                        is_all_downloaded = true;
                        break;
                    },
                    Err(e) => {
                        log::error!("다운로드 {} 실패: {}", id, e);
                        return Err(e.into());
                    }
                }
            }
        }
    }

    // 단계 5: 최종 플러시 및 종료 처리
    if let Some(mut f) = dest.take() {
        f.flush().await?;
    }

    // 다운로더 완료 플래그 설정
    if let Some(ref mut downloader) = DOWNLOADERS.lock().unwrap().get_mut(id) {
        downloader.finished = true;
    }

    // 성공 시: 지정된 시간 후 자동 삭제 스케줄 (설정된 경우)
    if is_all_downloaded {
        let id_del = id.to_string();
        if let Some(dur) = auto_del_dur {
            tokio::spawn(async move {
                tokio::time::sleep(dur).await;
                DOWNLOADERS.lock().unwrap().remove(&id_del);
            });
        }
    }
    Ok(is_all_downloaded)
}

/// 지정된 다운로드 작업의 현재 상태와 데이터 조회
pub fn get_download_data(id: &str) -> ResultType<DownloadData> {
    let downloaders = DOWNLOADERS.lock().unwrap();
    if let Some(downloader) = downloaders.get(id) {
        let downloaded_size = downloader.downloaded_size;
        let total_size = downloader.total_size.clone();
        let error = downloader.error.clone();
        // 전체 다운로드 완료되고 파일 경로가 없으면 메모리의 데이터 반환
        let data = if total_size.unwrap_or(0) == downloaded_size && downloader.path.is_none() {
            downloader.data.clone()
        } else {
            Vec::new()
        };
        let path = downloader.path.clone();
        let download_data = DownloadData {
            data,
            path,
            total_size,
            downloaded_size,
            error,
        };
        Ok(download_data)
    } else {
        bail!("다운로더를 찾을 수 없음")
    }
}

/// 진행 중인 다운로드 작업 취소
pub fn cancel(id: &str) {
    if let Some(downloader) = DOWNLOADERS.lock().unwrap().get(id) {
        // 취소 신호를 채널을 통해 전송 (수신자가 받지 못할 수도 있으므로 재시도 가능)
        let _ = downloader.tx_cancel.send(());
    }
}

/// 다운로드 작업을 맵에서 제거
pub fn remove(id: &str) {
    let _ = DOWNLOADERS.lock().unwrap().remove(id);
}
