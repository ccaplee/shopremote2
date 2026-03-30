use super::local_file::LocalFile;
use crate::{platform::unix::local_file::construct_file_list, ClipboardFile, CliprdrError};
use hbb_common::{
    bytes::{BufMut, BytesMut},
    log,
};
use parking_lot::Mutex;
use std::{path::PathBuf, sync::Arc, usize};

/// 로컬 파일 캐시입니다.
/// 파일 복사 중에는 이 값을 변경하면 안 됩니다.
/// CliprdrFileContentsRequest는 리스트의 파일 인덱스만 포함하므로,
/// 원격 쪽과 같은 순서로 파일 리스트를 유지해야 합니다.
/// 향후 CliprdrFileContentsRequest에 FileId 필드를 추가할 수도 있습니다.
lazy_static::lazy_static! {
    static ref CLIP_FILES: Arc<Mutex<ClipFiles>> = Default::default();
}

/// 파일 내용 요청의 종류를 나타내는 열거형입니다.
#[derive(Debug)]
enum FileContentsRequest {
    /// 파일 크기 요청
    Size {
        /// 스트림 ID
        stream_id: i32,
        /// 파일 인덱스
        file_idx: usize,
    },

    /// 파일 범위 요청
    Range {
        /// 스트림 ID
        stream_id: i32,
        /// 파일 인덱스
        file_idx: usize,
        /// 파일 오프셋
        offset: u64,
        /// 요청 길이
        length: u64,
    },
}

/// 클립보드의 파일들을 캐시하고 관리하는 구조체입니다.
#[derive(Default)]
struct ClipFiles {
    /// 파일 경로 목록
    files: Vec<String>,
    /// LocalFile 객체 목록
    file_list: Vec<LocalFile>,
    /// 첫 번째 일반 파일의 인덱스
    first_file_index: usize,
    /// 파일 리스트 PDU (Protocol Data Unit)
    files_pdu: Vec<u8>,
}

impl ClipFiles {
    /// 캐시된 파일 목록을 초기화합니다.
    fn clear(&mut self) {
        self.files.clear();
        self.file_list.clear();
        self.first_file_index = usize::MAX;
        self.files_pdu.clear();
    }

    /// 클립보드 파일 목록을 동기화하고 메타데이터를 수집합니다.
    fn sync_files(&mut self, clipboard_files: &[String]) -> Result<(), CliprdrError> {
        let clipboard_paths = clipboard_files
            .iter()
            .map(|s| PathBuf::from(s))
            .collect::<Vec<_>>();
        self.file_list = construct_file_list(&clipboard_paths)?;
        self.first_file_index = self
            .file_list
            .iter()
            .position(|f| !f.path.is_dir())
            .unwrap_or(usize::MAX);
        self.files = clipboard_files.to_vec();
        Ok(())
    }

    /// 파일 리스트를 PDU 형식으로 빌드합니다.
    fn build_file_list_pdu(&mut self) {
        let mut data = BytesMut::with_capacity(4 + 592 * self.file_list.len());
        data.put_u32_le(self.file_list.len() as u32);
        for file in self.file_list.iter() {
            data.put(file.as_bin().as_slice());
        }
        self.files_pdu = data.to_vec()
    }

    fn get_files_for_audit(&self, request: &FileContentsRequest) -> Option<ClipboardFile> {
        if let FileContentsRequest::Range {
            file_idx, offset, ..
        } = request
        {
            if *file_idx == self.first_file_index && *offset == 0 {
                let files: Vec<(String, u64)> = self
                    .file_list
                    .iter()
                    .filter_map(|f| {
                        if f.path.is_file() {
                            Some((f.path.to_string_lossy().to_string(), f.size))
                        } else {
                            None
                        }
                    })
                    .collect::<_>();
                if files.is_empty() {
                    return None;
                } else {
                    return Some(ClipboardFile::Files { files });
                }
            }
        }
        None
    }

    fn serve_file_contents(
        &mut self,
        conn_id: i32,
        request: FileContentsRequest,
    ) -> Result<ClipboardFile, CliprdrError> {
        let (file_idx, file_contents_resp) = match request {
            FileContentsRequest::Size {
                stream_id,
                file_idx,
            } => {
                log::debug!("file contents (size) requested from conn: {}", conn_id);
                let Some(file) = self.file_list.get(file_idx) else {
                    log::error!(
                        "invalid file index {} requested from conn: {}",
                        file_idx,
                        conn_id
                    );
                    return Err(CliprdrError::InvalidRequest {
                        description: format!(
                            "invalid file index {} requested from conn: {}",
                            file_idx, conn_id
                        ),
                    });
                };

                log::debug!(
                    "conn {} requested file-{}: {}",
                    conn_id,
                    file_idx,
                    file.name
                );

                let size = file.size;
                (
                    file_idx,
                    ClipboardFile::FileContentsResponse {
                        msg_flags: 0x1,
                        stream_id,
                        requested_data: size.to_le_bytes().to_vec(),
                    },
                )
            }
            FileContentsRequest::Range {
                stream_id,
                file_idx,
                offset,
                length,
            } => {
                log::debug!(
                    "file contents (range from {} length {}) request from conn: {}",
                    offset,
                    length,
                    conn_id
                );
                let Some(file) = self.file_list.get_mut(file_idx) else {
                    log::error!(
                        "invalid file index {} requested from conn: {}",
                        file_idx,
                        conn_id
                    );
                    return Err(CliprdrError::InvalidRequest {
                        description: format!(
                            "invalid file index {} requested from conn: {}",
                            file_idx, conn_id
                        ),
                    });
                };
                log::debug!(
                    "conn {} requested file-{}: {}",
                    conn_id,
                    file_idx,
                    file.name
                );

                if offset > file.size {
                    log::error!("invalid reading offset requested from conn: {}", conn_id);
                    return Err(CliprdrError::InvalidRequest {
                        description: format!(
                            "invalid reading offset requested from conn: {}",
                            conn_id
                        ),
                    });
                }
                let read_size = if offset + length > file.size {
                    file.size - offset
                } else {
                    length
                };

                let mut buf = vec![0u8; read_size as usize];

                file.read_exact_at(&mut buf, offset)?;

                (
                    file_idx,
                    ClipboardFile::FileContentsResponse {
                        msg_flags: 0x1,
                        stream_id,
                        requested_data: buf,
                    },
                )
            }
        };

        log::debug!("file contents sent to conn: {}", conn_id);
        // hot reload next file
        for next_file in self.file_list.iter_mut().skip(file_idx + 1) {
            if !next_file.is_dir {
                next_file.load_handle()?;
                break;
            }
        }
        Ok(file_contents_resp)
    }
}

#[inline]
pub fn clear_files() {
    CLIP_FILES.lock().clear();
}

pub fn read_file_contents(
    conn_id: i32,
    stream_id: i32,
    list_index: i32,
    dw_flags: i32,
    n_position_low: i32,
    n_position_high: i32,
    cb_requested: i32,
) -> Vec<Result<ClipboardFile, CliprdrError>> {
    let fcr = if dw_flags == 0x1 {
        FileContentsRequest::Size {
            stream_id,
            file_idx: list_index as usize,
        }
    } else if dw_flags == 0x2 {
        let offset = (n_position_high as u64) << 32 | n_position_low as u64;
        let length = cb_requested as u64;

        FileContentsRequest::Range {
            stream_id,
            file_idx: list_index as usize,
            offset,
            length,
        }
    } else {
        return vec![Err(CliprdrError::InvalidRequest {
            description: format!("got invalid FileContentsRequest, dw_flats: {dw_flags}"),
        })];
    };

    let mut clip_files = CLIP_FILES.lock();
    let mut res = vec![];
    if let Some(files_res) = clip_files.get_files_for_audit(&fcr) {
        res.push(Ok(files_res));
    }
    res.push(clip_files.serve_file_contents(conn_id, fcr));
    res
}

pub fn sync_files(files: &[String]) -> Result<(), CliprdrError> {
    let mut files_lock = CLIP_FILES.lock();
    if files_lock.files == files {
        return Ok(());
    }
    files_lock.sync_files(files)?;
    Ok(files_lock.build_file_list_pdu())
}

pub fn get_file_list_pdu() -> Vec<u8> {
    CLIP_FILES.lock().files_pdu.clone()
}
