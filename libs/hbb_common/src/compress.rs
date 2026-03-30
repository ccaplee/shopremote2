use std::{cell::RefCell, io};
use zstd::bulk::Compressor;

/// ZSTD 압축 라이브러리 설정
/// 지원하는 압축 레벨: 1 ~ ZSTD_maxCLevel() (현재 22)
/// 기본 레벨: ZSTD_CLEVEL_DEFAULT (3)
/// 값 0은 기본값을 의미합니다.
/// 스레드-로컬 변수로 각 스레드가 독립적인 압축기를 사용합니다.
thread_local! {
    static COMPRESSOR: RefCell<io::Result<Compressor<'static>>> = RefCell::new(Compressor::new(crate::config::COMPRESS_LEVEL));
}

/// 데이터를 ZSTD 알고리즘으로 압축합니다.
/// 압축 실패 시 빈 벡터를 반환하고 디버그 로그를 출력합니다.
pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    COMPRESSOR.with(|c| {
        if let Ok(mut c) = c.try_borrow_mut() {
            match &mut *c {
                Ok(c) => match c.compress(data) {
                    Ok(res) => out = res,
                    Err(err) => {
                        crate::log::debug!("Failed to compress: {}", err);
                    }
                },
                Err(err) => {
                    crate::log::debug!("Failed to get compressor: {}", err);
                }
            }
        }
    });
    out
}

/// ZSTD로 압축된 데이터를 복호화합니다.
/// 복호화 실패 시 빈 벡터를 반환합니다.
pub fn decompress(data: &[u8]) -> Vec<u8> {
    zstd::decode_all(data).unwrap_or_default()
}
