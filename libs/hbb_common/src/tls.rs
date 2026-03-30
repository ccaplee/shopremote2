use std::{collections::HashMap, sync::RwLock};

use crate::config::allow_insecure_tls_fallback;

/// TLS 구현 타입을 나타냅니다.
#[derive(Debug, Clone, Copy)]
pub enum TlsType {
    // 평문 HTTP 통신 (TLS 없음)
    Plain,
    // Native TLS (운영체제 기본 TLS 라이브러리 사용)
    NativeTls,
    // Rustls (Rust 기본 TLS 라이브러리)
    Rustls,
}

lazy_static::lazy_static! {
    /// URL 도메인별로 선택된 TLS 타입을 캐싱합니다.
    static ref URL_TLS_TYPE: RwLock<HashMap<String, TlsType>> = RwLock::new(HashMap::new());
    /// URL 도메인별로 무효한 인증서 수락 여부를 캐싱합니다.
    static ref URL_TLS_DANGER_ACCEPT_INVALID_CERTS: RwLock<HashMap<String, bool>> = RwLock::new(HashMap::new());
}

/// URL이 평문 통신(TLS 없음)을 사용하는지 확인합니다.
#[inline]
pub fn is_plain(url: &str) -> bool {
    url.starts_with("ws://") || url.starts_with("http://")
}

/// URL에서 도메인과 포트를 추출합니다.
/// 예시:
/// - "https://example.com/path" -> "example.com"
/// - "https://example.com:8080/path" -> "example.com:8080"
/// - "https://user:pass@example.com" -> "example.com"
/// 테스트에서 더 많은 예시를 확인할 수 있습니다.
#[inline]
fn get_domain_and_port_from_url(url: &str) -> &str {
    // Remove scheme (e.g., http://, https://, ws://, wss://)
    let scheme_end = url.find("://").map(|pos| pos + 3).unwrap_or(0);
    let url2 = &url[scheme_end..];
    // If userinfo is present, domain is after last '@'
    let after_at = match url2.rfind('@') {
        Some(pos) => &url2[pos + 1..],
        None => url2,
    };
    // Find the end of domain (before '/' or '?')
    let domain_end = after_at.find(&['/', '?'][..]).unwrap_or(after_at.len());
    &after_at[..domain_end]
}

/// TLS 캐시를 업데이트합니다 (삽입 또는 업데이트).
/// 평문 URL은 캐싱하지 않습니다.
#[inline]
pub fn upsert_tls_cache(url: &str, tls_type: TlsType, danger_accept_invalid_cert: bool) {
    if is_plain(url) {
        return;
    }

    let domain_port = get_domain_and_port_from_url(url);
    // 락이 즉시 해제되도록 중괄호 사용
    {
        URL_TLS_TYPE
            .write()
            .unwrap()
            .insert(domain_port.to_string(), tls_type);
    }
    {
        URL_TLS_DANGER_ACCEPT_INVALID_CERTS
            .write()
            .unwrap()
            .insert(domain_port.to_string(), danger_accept_invalid_cert);
    }
}

/// TLS 캐시를 비웁니다.
#[inline]
pub fn reset_tls_cache() {
    // 락이 즉시 해제되도록 중괄호 사용
    {
        URL_TLS_TYPE.write().unwrap().clear();
    }
    {
        URL_TLS_DANGER_ACCEPT_INVALID_CERTS.write().unwrap().clear();
    }
}

/// URL의 캐시된 TLS 타입을 반환합니다.
/// 평문 URL은 TlsType::Plain을 반환합니다.
#[inline]
pub fn get_cached_tls_type(url: &str) -> Option<TlsType> {
    if is_plain(url) {
        return Some(TlsType::Plain);
    }
    let domain_port = get_domain_and_port_from_url(url);
    URL_TLS_TYPE.read().unwrap().get(domain_port).cloned()
}

/// URL의 캐시된 무효 인증서 수락 여부를 반환합니다.
/// 보안 설정에 의해 비활성화되면 항상 false를 반환합니다.
#[inline]
pub fn get_cached_tls_accept_invalid_cert(url: &str) -> Option<bool> {
    if !allow_insecure_tls_fallback() {
        return Some(false);
    }

    if is_plain(url) {
        return Some(false);
    }
    let domain_port = get_domain_and_port_from_url(url);
    URL_TLS_DANGER_ACCEPT_INVALID_CERTS
        .read()
        .unwrap()
        .get(domain_port)
        .cloned()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_domain_and_port_from_url() {
        for (url, expected_domain_port) in vec![
            ("http://example.com", "example.com"),
            ("https://example.com", "example.com"),
            ("ws://example.com/path", "example.com"),
            ("wss://example.com:8080/path", "example.com:8080"),
            ("https://user:pass@example.com", "example.com"),
            ("https://example.com?query=param", "example.com"),
            ("https://example.com:8443?query=param", "example.com:8443"),
            ("ftp://example.com/resource", "example.com"), // ftp scheme
            ("example.com/path", "example.com"),           // no scheme
            ("example.com:8080/path", "example.com:8080"),
        ] {
            let domain_port = get_domain_and_port_from_url(url);
            assert_eq!(domain_port, expected_domain_port);
        }
    }
}
