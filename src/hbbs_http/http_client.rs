use hbb_common::{
    async_recursion::async_recursion,
    config::{Config, Socks5Server},
    log::{self, info},
    proxy::{Proxy, ProxyScheme},
    tls::{
        get_cached_tls_accept_invalid_cert, get_cached_tls_type, is_plain, upsert_tls_cache,
        TlsType,
    },
};
use reqwest::{blocking::Client as SyncClient, Client as AsyncClient};

/// HTTP 클라이언트를 설정하는 매크로
/// TLS 타입, 인증서 검증 설정, 프록시 설정을 적용
macro_rules! configure_http_client {
    ($builder:expr, $tls_type:expr, $danger_accept_invalid_cert:expr, $Client: ty) => {{
        // GitHub issue #11569 참조
        // reqwest 문서: https://docs.rs/reqwest/latest/reqwest/struct.ClientBuilder.html#method.no_proxy
        let mut builder = $builder.no_proxy();

        // TLS 타입에 따라 설정
        match $tls_type {
            TlsType::Plain => {
                // 평문 HTTP (TLS 없음)
            }
            TlsType::NativeTls => {
                // 네이티브 TLS 사용 (시스템 인증서 저장소)
                builder = builder.use_native_tls();
                if $danger_accept_invalid_cert {
                    // 유효하지 않은 인증서 허용
                    builder = builder.danger_accept_invalid_certs(true);
                }
            }
            TlsType::Rustls => {
                // Rustls 기반 TLS 사용 (순수 Rust 구현)
                #[cfg(any(target_os = "android", target_os = "ios"))]
                match hbb_common::verifier::client_config($danger_accept_invalid_cert) {
                    Ok(client_config) => {
                        builder = builder.use_preconfigured_tls(client_config);
                    }
                    Err(e) => {
                        hbb_common::log::error!("클라이언트 설정 획득 실패: {}", e);
                    }
                }
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                {
                    builder = builder.use_rustls_tls();
                    if $danger_accept_invalid_cert {
                        builder = builder.danger_accept_invalid_certs(true);
                    }
                }
            }
        }

        // 프록시 설정 적용
        let client = if let Some(conf) = Config::get_socks() {
            let proxy_result = Proxy::from_conf(&conf, None);

            match proxy_result {
                Ok(proxy) => {
                    // 프록시 타입에 따라 설정
                    let proxy_setup = match &proxy.intercept {
                        ProxyScheme::Http { host, .. } => {
                            reqwest::Proxy::all(format!("http://{}", host))
                        }
                        ProxyScheme::Https { host, .. } => {
                            reqwest::Proxy::all(format!("https://{}", host))
                        }
                        ProxyScheme::Socks5 { addr, .. } => {
                            reqwest::Proxy::all(&format!("socks5://{}", addr))
                        }
                    };

                    match proxy_setup {
                        Ok(mut p) => {
                            // 프록시 인증 설정
                            if let Some(auth) = proxy.intercept.maybe_auth() {
                                if !auth.username().is_empty() && !auth.password().is_empty() {
                                    p = p.basic_auth(auth.username(), auth.password());
                                }
                            }
                            builder = builder.proxy(p);
                            builder.build().unwrap_or_else(|e| {
                                info!("프록시된 클라이언트 생성 실패: {}", e);
                                <$Client>::new()
                            })
                        }
                        Err(e) => {
                            info!("프록시 설정 실패: {}", e);
                            <$Client>::new()
                        }
                    }
                }
                Err(e) => {
                    info!("프록시 구성 실패: {}", e);
                    <$Client>::new()
                }
            }
        } else {
            // 프록시 없음 - 기본 클라이언트 생성
            builder.build().unwrap_or_else(|e| {
                info!("클라이언트 생성 실패: {}", e);
                <$Client>::new()
            })
        };

        client
    }};
}

/// 지정된 TLS 설정으로 동기 HTTP 클라이언트 생성
pub fn create_http_client(tls_type: TlsType, danger_accept_invalid_cert: bool) -> SyncClient {
    let builder = SyncClient::builder();
    configure_http_client!(builder, tls_type, danger_accept_invalid_cert, SyncClient)
}

/// 지정된 TLS 설정으로 비동기 HTTP 클라이언트 생성
pub fn create_http_client_async(
    tls_type: TlsType,
    danger_accept_invalid_cert: bool,
) -> AsyncClient {
    let builder = AsyncClient::builder();
    configure_http_client!(builder, tls_type, danger_accept_invalid_cert, AsyncClient)
}

/// URL이 평문 HTTP인 경우 프록시 주소 반환, 그 외에는 원본 URL 반환
/// TLS 타입 감지 시 사용할 실제 URL을 결정
pub fn get_url_for_tls<'a>(url: &'a str, proxy_conf: &'a Option<Socks5Server>) -> &'a str {
    if is_plain(url) {
        if let Some(conf) = proxy_conf {
            if conf.proxy.starts_with("https://") {
                return &conf.proxy;
            }
        }
    }
    url
}

/// URL을 기반으로 적절한 TLS 설정으로 동기 HTTP 클라이언트 생성
/// 캐시된 TLS 타입을 사용하거나, 자동으로 감지하여 재시도
pub fn create_http_client_with_url(url: &str) -> SyncClient {
    let proxy_conf = Config::get_socks();
    let tls_url = get_url_for_tls(url, &proxy_conf);
    // 캐시된 TLS 타입 조회
    let tls_type = get_cached_tls_type(tls_url);
    let is_tls_type_cached = tls_type.is_some();
    let tls_type = tls_type.unwrap_or(TlsType::Rustls);
    // 캐시된 인증서 검증 설정 조회
    let tls_danger_accept_invalid_cert = get_cached_tls_accept_invalid_cert(tls_url);
    create_http_client_with_url_(
        url,
        tls_url,
        tls_type,
        is_tls_type_cached,
        tls_danger_accept_invalid_cert,
        tls_danger_accept_invalid_cert,
    )
}

/// 동기 HTTP 클라이언트 생성 (내부 재귀 함수)
/// TLS 연결 실패 시 다른 TLS 타입이나 인증서 설정으로 재시도
fn create_http_client_with_url_(
    url: &str,
    tls_url: &str,
    tls_type: TlsType,
    is_tls_type_cached: bool,
    danger_accept_invalid_cert: Option<bool>,
    original_danger_accept_invalid_cert: Option<bool>,
) -> SyncClient {
    let mut client = create_http_client(tls_type, danger_accept_invalid_cert.unwrap_or(false));

    // 캐시된 설정이 완전히 있으면 그대로 사용
    if is_tls_type_cached && original_danger_accept_invalid_cert.is_some() {
        return client;
    }

    // HEAD 요청으로 연결 테스트
    if let Err(e) = client.head(url).send() {
        if e.is_request() {
            // 요청 오류 - TLS 설정 문제일 가능성
            match (tls_type, is_tls_type_cached, danger_accept_invalid_cert) {
                // Rustls 연결 실패: 유효하지 않은 인증서 무시하고 재시도
                (TlsType::Rustls, _, None) => {
                    log::warn!(
                        "서버 {} rustls-tls 연결 실패: {:?}, 유효하지 않은 인증서 무시 시도",
                        tls_url,
                        e
                    );
                    client = create_http_client_with_url_(
                        url,
                        tls_url,
                        tls_type,
                        is_tls_type_cached,
                        Some(true),
                        original_danger_accept_invalid_cert,
                    );
                }
                // Rustls 실패 후 유효하지 않은 인증서 시도도 실패: native-tls로 전환
                (TlsType::Rustls, false, Some(_)) => {
                    log::warn!(
                        "서버 {} rustls-tls 연결 실패: {:?}, native-tls 시도",
                        tls_url,
                        e
                    );
                    client = create_http_client_with_url_(
                        url,
                        tls_url,
                        TlsType::NativeTls,
                        is_tls_type_cached,
                        original_danger_accept_invalid_cert,
                        original_danger_accept_invalid_cert,
                    );
                }
                // NativeTls 연결 실패: 유효하지 않은 인증서 무시하고 재시도
                (TlsType::NativeTls, _, None) => {
                    log::warn!(
                        "서버 {} native-tls 연결 실패: {:?}, 유효하지 않은 인증서 무시 시도",
                        tls_url,
                        e
                    );
                    client = create_http_client_with_url_(
                        url,
                        tls_url,
                        tls_type,
                        is_tls_type_cached,
                        Some(true),
                        original_danger_accept_invalid_cert,
                    );
                }
                // 다른 모든 경우: 연결 불가
                _ => {
                    log::error!(
                        "서버 {} {:?} 연결 실패, 오류: {:?}.",
                        tls_url,
                        tls_type,
                        e
                    );
                }
            }
        } else {
            // 네트워크 오류 등 다른 종류의 오류
            log::warn!(
                "서버 {} {:?} 연결 실패, 오류: {}.",
                tls_url,
                tls_type,
                e
            );
        }
    } else {
        // 연결 성공 - TLS 설정 캐싱
        log::info!(
            "서버 {} {:?} 연결 성공",
            tls_url,
            tls_type
        );
        upsert_tls_cache(
            tls_url,
            tls_type,
            danger_accept_invalid_cert.unwrap_or(false),
        );
    }
    client
}

/// URL을 기반으로 적절한 TLS 설정으로 비동기 HTTP 클라이언트 생성
/// 캐시된 TLS 타입을 사용하거나, 자동으로 감지하여 재시도
pub async fn create_http_client_async_with_url(url: &str) -> AsyncClient {
    let proxy_conf = Config::get_socks();
    let tls_url = get_url_for_tls(url, &proxy_conf);
    // 캐시된 TLS 타입 조회
    let tls_type = get_cached_tls_type(tls_url);
    let is_tls_type_cached = tls_type.is_some();
    let tls_type = tls_type.unwrap_or(TlsType::Rustls);
    // 캐시된 인증서 검증 설정 조회
    let danger_accept_invalid_cert = get_cached_tls_accept_invalid_cert(tls_url);
    create_http_client_async_with_url_(
        url,
        tls_url,
        tls_type,
        is_tls_type_cached,
        danger_accept_invalid_cert,
        danger_accept_invalid_cert,
    )
    .await
}

/// 비동기 HTTP 클라이언트 생성 (내부 재귀 함수)
/// TLS 연결 실패 시 다른 TLS 타입이나 인증서 설정으로 재시도
#[async_recursion]
async fn create_http_client_async_with_url_(
    url: &str,
    tls_url: &str,
    tls_type: TlsType,
    is_tls_type_cached: bool,
    danger_accept_invalid_cert: Option<bool>,
    original_danger_accept_invalid_cert: Option<bool>,
) -> AsyncClient {
    let mut client =
        create_http_client_async(tls_type, danger_accept_invalid_cert.unwrap_or(false));

    // 캐시된 설정이 완전히 있으면 그대로 사용
    if is_tls_type_cached && original_danger_accept_invalid_cert.is_some() {
        return client;
    }

    // HEAD 요청으로 연결 테스트
    if let Err(e) = client.head(url).send().await {
        // 요청 오류 - TLS 설정 문제일 가능성
        match (tls_type, is_tls_type_cached, danger_accept_invalid_cert) {
            // Rustls 연결 실패: 유효하지 않은 인증서 무시하고 재시도
            (TlsType::Rustls, _, None) => {
                log::warn!(
                    "서버 {} rustls-tls 연결 실패: {:?}, 유효하지 않은 인증서 무시 시도",
                    tls_url,
                    e
                );
                client = create_http_client_async_with_url_(
                    url,
                    tls_url,
                    tls_type,
                    is_tls_type_cached,
                    Some(true),
                    original_danger_accept_invalid_cert,
                )
                .await;
            }
            // Rustls 실패 후 유효하지 않은 인증서 시도도 실패: native-tls로 전환
            (TlsType::Rustls, false, Some(_)) => {
                log::warn!(
                    "서버 {} rustls-tls 연결 실패: {:?}, native-tls 시도",
                    tls_url,
                    e
                );
                client = create_http_client_async_with_url_(
                    url,
                    tls_url,
                    TlsType::NativeTls,
                    is_tls_type_cached,
                    original_danger_accept_invalid_cert,
                    original_danger_accept_invalid_cert,
                )
                .await;
            }
            // NativeTls 연결 실패: 유효하지 않은 인증서 무시하고 재시도
            (TlsType::NativeTls, _, None) => {
                log::warn!(
                    "서버 {} native-tls 연결 실패: {:?}, 유효하지 않은 인증서 무시 시도",
                    tls_url,
                    e
                );
                client = create_http_client_async_with_url_(
                    url,
                    tls_url,
                    tls_type,
                    is_tls_type_cached,
                    Some(true),
                    original_danger_accept_invalid_cert,
                )
                .await;
            }
            // 다른 모든 경우: 연결 불가
            _ => {
                log::error!(
                    "서버 {} {:?} 연결 실패, 오류: {:?}.",
                    tls_url,
                    tls_type,
                    e
                );
            }
        }
    } else {
        // 연결 성공 - TLS 설정 캐싱
        log::info!(
            "서버 {} {:?} 연결 성공",
            tls_url,
            tls_type
        );
        upsert_tls_cache(
            tls_url,
            tls_type,
            danger_accept_invalid_cert.unwrap_or(false),
        );
    }
    client
}
