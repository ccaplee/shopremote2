use super::{FLAGS_FD_ATTRIBUTES, FLAGS_FD_LAST_WRITE, FLAGS_FD_UNIX_MODE, LDAP_EPOCH_DELTA};
use crate::CliprdrError;
use hbb_common::{
    bytes::{Buf, Bytes},
    log,
};
use serde_derive::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};
use utf16string::WStr;

#[cfg(target_os = "linux")]
pub type Inode = u64;

/// 파일의 종류를 나타내는 열거형입니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileType {
    /// 일반 파일
    File,
    /// 디렉토리
    Directory,
    // TODO: 심볼릭 링크 지원
    Symlink,
}

/// 읽기 전용 권한
pub const PERM_READ: u16 = 0o444;
/// 읽기 및 쓰기 권한
pub const PERM_RW: u16 = 0o644;
/// 소유자만 읽기 및 읽기 전용
pub const PERM_SELF_RO: u16 = 0o400;
/// 소유자는 읽기, 쓰기, 실행. 그룹과 다른 사용자는 읽기, 실행.
pub const PERM_RWX: u16 = 0o755;
#[allow(dead_code)]
/// 파일 이름의 최대 길이
pub const MAX_NAME_LEN: usize = 255;

/// 원격 클립보드에서 받은 파일의 메타데이터를 나타내는 구조체입니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDescription {
    /// 연결 ID
    pub conn_id: i32,
    /// 파일 경로
    pub name: PathBuf,
    /// 파일 종류 (파일/디렉토리)
    pub kind: FileType,
    /// 마지막 접근 시간
    pub atime: SystemTime,
    /// 마지막 수정 시간
    pub last_modified: SystemTime,
    /// 마지막 메타데이터 변경 시간
    pub last_metadata_changed: SystemTime,
    /// 생성 시간
    pub creation_time: SystemTime,
    /// 파일 크기 (바이트)
    pub size: u64,
    /// 파일 권한 (Unix 모드)
    pub perm: u16,
}

impl FileDescription {
    /// 바이트 스트림에서 파일 설명자를 파싱합니다.
    /// Windows의 FILEDESCRIPTORW 구조체 형식을 파싱합니다.
    fn parse_file_descriptor(
        bytes: &mut Bytes,
        conn_id: i32,
    ) -> Result<FileDescription, CliprdrError> {
        let flags = bytes.get_u32_le();
        // 예약된 32바이트를 건너뜁니다
        bytes.advance(32);
        let attributes = bytes.get_u32_le();

        // 원래 사양에서는 16바이트가 예약되어 있습니다.
        // 마지막 4바이트를 파일 모드 저장소로 사용합니다.
        // 예약된 12바이트를 건너뜁니다.
        bytes.advance(12);
        let perm = bytes.get_u32_le() as u16;

        // 1601-01-01 00:00:00부터의 마지막 쓰기 시간, 100ns 단위
        let last_write_time = bytes.get_u64_le();
        // 파일 크기
        let file_size_high = bytes.get_u32_le();
        let file_size_low = bytes.get_u32_le();
        // UTF-16 파일 이름, 이중 \0 종료, 520바이트 블록 단위
        // 다른 포인터로 읽고 메인 포인터를 진행합니다.
        let block = bytes.clone();
        bytes.advance(520);

        let block = &block[..520];
        let wstr = WStr::from_utf16le(block).map_err(|e| {
            log::error!("cannot convert file descriptor path: {:?}", e);
            CliprdrError::ConversionFailure
        })?;

        let from_unix = flags & FLAGS_FD_UNIX_MODE != 0;

        // 파일이 유효한 속성을 가지는지 확인합니다.
        let valid_attributes = flags & FLAGS_FD_ATTRIBUTES != 0;
        if !valid_attributes {
            return Err(CliprdrError::InvalidRequest {
                description: "file description must have valid attributes".to_string(),
            });
        }

        // TODO: normal, hidden, system, readonly, archive 등을 확인합니다.
        let directory = attributes & 0x10 != 0;
        let normal = attributes == 0x80;
        let hidden = attributes & 0x02 != 0;
        let readonly = attributes & 0x01 != 0;

        let perm = if from_unix {
            // as is
            perm
            // cannot set as is...
        } else if normal {
            PERM_RWX
        } else if readonly {
            PERM_READ
        } else if hidden {
            PERM_SELF_RO
        } else if directory {
            PERM_RWX
        } else {
            PERM_RW
        };

        let kind = if directory {
            FileType::Directory
        } else {
            FileType::File
        };

        // TODO: `let valid_size = flags & FLAGS_FD_SIZE != 0;`를 사용해야 합니다.
        // Windows와의 호환성을 위해 `true`를 사용합니다.
        // let valid_size = flags & FLAGS_FD_SIZE != 0;
        let valid_size = true;
        let size = if valid_size {
            ((file_size_high as u64) << 32) + file_size_low as u64
        } else {
            0
        };

        // 마지막 쓰기 시간이 유효한지 확인합니다.
        let valid_write_time = flags & FLAGS_FD_LAST_WRITE != 0;
        let last_modified = if valid_write_time && last_write_time >= LDAP_EPOCH_DELTA {
            // Windows epoch (1601-01-01)에서 Unix epoch (1970-01-01)로 변환합니다.
            let last_write_time = (last_write_time - LDAP_EPOCH_DELTA) * 100;
            let last_write_time = Duration::from_nanos(last_write_time);
            SystemTime::UNIX_EPOCH + last_write_time
        } else {
            SystemTime::UNIX_EPOCH
        };

        let name = wstr.to_utf8().replace('\\', "/");
        let name = PathBuf::from(name.trim_end_matches('\0'));

        let desc = FileDescription {
            conn_id,
            name,
            kind,
            atime: last_modified,
            last_modified,
            last_metadata_changed: last_modified,
            creation_time: last_modified,
            size,
            perm,
        };

        Ok(desc)
    }

    /// 포맷 데이터 응답 PDU에서 파일 설명자들을 파싱합니다.
    /// CSPTR_FILEDESCRIPTORW 포맷 데이터를 포함합니다.
    pub fn parse_file_descriptors(
        file_descriptor_pdu: Vec<u8>,
        conn_id: i32,
    ) -> Result<Vec<Self>, CliprdrError> {
        let mut data = Bytes::from(file_descriptor_pdu);
        if data.remaining() < 4 {
            return Err(CliprdrError::InvalidRequest {
                description: "file descriptor request with infficient length".to_string(),
            });
        }

        let count = data.get_u32_le() as usize;
        if data.remaining() == 0 && count == 0 {
            return Ok(Vec::new());
        }

        if data.remaining() != 592 * count {
            return Err(CliprdrError::InvalidRequest {
                description: "file descriptor request with invalid length".to_string(),
            });
        }

        let mut files = Vec::with_capacity(count);
        for _ in 0..count {
            let desc = Self::parse_file_descriptor(&mut data, conn_id)?;
            files.push(desc);
        }

        Ok(files)
    }
}
