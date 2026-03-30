use dashmap::DashMap;
use lazy_static::lazy_static;

mod filetype;
pub use filetype::{FileDescription, FileType};
/// Linux에서 파일 붙여넣기를 위해 FUSE를 사용합니다.
#[cfg(target_os = "linux")]
pub mod fuse;
#[cfg(target_os = "macos")]
pub mod macos;

pub mod local_file;
pub mod serv_files;

/// 파일의 유효한 속성을 가지고 있음을 나타냅니다.
pub const FLAGS_FD_ATTRIBUTES: u32 = 0x04;
/// 파일 크기가 유효함을 나타냅니다.
pub const FLAGS_FD_SIZE: u32 = 0x40;
/// 마지막 쓰기 시간이 유효함을 나타냅니다.
pub const FLAGS_FD_LAST_WRITE: u32 = 0x20;
/// 진행도를 표시합니다.
pub const FLAGS_FD_PROGRESSUI: u32 = 0x4000;
/// Unix에서 전송되었으며 파일 모드를 포함합니다.
/// 참고: 이 플래그는 Windows에서는 사용되지 않습니다.
pub const FLAGS_FD_UNIX_MODE: u32 = 0x08;

// 실제 포맷 ID가 아니라 단순 자리표시자입니다.
pub const FILEDESCRIPTOR_FORMAT_ID: i32 = 49334;
pub const FILEDESCRIPTORW_FORMAT_NAME: &str = "FileGroupDescriptorW";
// 실제 포맷 ID가 아니라 단순 자리표시자입니다.
pub const FILECONTENTS_FORMAT_ID: i32 = 49267;
pub const FILECONTENTS_FORMAT_NAME: &str = "FileContents";

/// FUSE 블록 크기. FileContentsRequest 비동기 요청 크기에 맞춰 정렬됩니다.
pub(crate) const BLOCK_SIZE: u32 = 4 * 1024 * 1024;

// Microsoft에서 사용하는 에포크 시작점입니다.
// 1601-01-01 00:00:00 + LDAP_EPOCH_DELTA*(100 ns) = 1970-01-01 00:00:00
const LDAP_EPOCH_DELTA: u64 = 116444772610000000;

/// 원격 포맷 ID를 로컬 포맷 이름에 매핑하는 맵입니다.
lazy_static! {
    static ref REMOTE_FORMAT_MAP: DashMap<i32, String> = DashMap::from_iter(
        [
            (
                FILEDESCRIPTOR_FORMAT_ID,
                FILEDESCRIPTORW_FORMAT_NAME.to_string()
            ),
            (FILECONTENTS_FORMAT_ID, FILECONTENTS_FORMAT_NAME.to_string())
        ]
        .iter()
        .cloned()
    );
}

/// 원격 포맷 ID에 해당하는 로컬 포맷 이름을 조회합니다.
#[inline]
pub fn get_local_format(remote_id: i32) -> Option<String> {
    REMOTE_FORMAT_MAP.get(&remote_id).map(|s| s.clone())
}
