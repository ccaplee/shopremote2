// 압축 모듈 - 데이터 압축 기능 제공
pub mod compress;
// 플랫폼별 특화 모듈 - 다양한 OS에 대한 플랫폼 특정 기능
pub mod platform;
// 프로토콜 버퍼 메시지 정의
pub mod protos;
pub use bytes;
use config::Config;
pub use futures;
pub use protobuf;
pub use protos::message as message_proto;
pub use protos::rendezvous as rendezvous_proto;
use serde_derive::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{self, BufRead},
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4},
    path::Path,
    time::{self, SystemTime, UNIX_EPOCH},
};
pub use tokio;
pub use tokio_util;
// 프록시 지원 모듈 - HTTP/HTTPS/SOCKS5 프록시 연결
pub mod proxy;
// 소켓 클라이언트 모듈 - 소켓 기반 통신 유틸리티
pub mod socket_client;
// TCP 통신 모듈 - TCP 프로토콜 처리
pub mod tcp;
// UDP 통신 모듈 - UDP 프로토콜 처리
pub mod udp;
pub use env_logger;
pub use log;
// 바이트 인코딩/디코딩 코덱 모듈
pub mod bytes_codec;
pub use anyhow::{self, bail};
pub use futures_util;
// 설정 관리 모듈 - 애플리케이션 전역 설정
pub mod config;
// 파일 시스템 작업 모듈 - 파일 I/O 유틸리티
pub mod fs;
// 메모리 모듈
pub mod mem;
pub use lazy_static;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use mac_address;
pub use rand;
pub use regex;
pub use sodiumoxide;
pub use tokio_socks;
pub use tokio_socks::IntoTargetAddr;
pub use tokio_socks::TargetAddr;
// 비밀번호 보안 모듈 - 암호화/복호화 및 비밀번호 생성
pub mod password_security;
pub use chrono;
pub use directories_next;
pub use libc;
// 키보드 모드 처리 모듈
pub mod keyboard;
pub use base64;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use dlopen;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use machine_uid;
pub use serde_derive;
pub use serde_json;
pub use sha2;
pub use sysinfo;
pub use thiserror;
pub use toml;
pub use uuid;
// 기기 지문 모듈 - 기기 고유 정보 생성 및 검증
pub mod fingerprint;
pub use flexi_logger;
// 스트림 통합 모듈 - WebSocket/TCP/WebRTC 통합 인터페이스
pub mod stream;
pub mod websocket;
#[cfg(feature = "webrtc")]
pub mod webrtc;
#[cfg(any(target_os = "android", target_os = "ios"))]
pub use rustls_platform_verifier;
pub use stream::Stream;
pub use whoami;
// TLS/SSL 인증서 검증 모듈
pub mod tls;
// TLS 증명서 검증자 모듈 - 안전하고 위험한 검증 옵션
pub mod verifier;
pub use async_recursion;
#[cfg(target_os = "linux")]
pub use users;
pub use libloading;
#[cfg(target_os = "linux")]
pub use x11;

// 세션 ID 타입 정의 - UUID 기반 고유한 세션 식별자
pub type SessionID = uuid::Uuid;

#[inline]
pub async fn sleep(sec: f32) {
    tokio::time::sleep(time::Duration::from_secs_f32(sec)).await;
}

#[macro_export]
macro_rules! allow_err {
    ($e:expr) => {
        if let Err(err) = $e {
            log::debug!(
                "{:?}, {}:{}:{}:{}",
                err,
                module_path!(),
                file!(),
                line!(),
                column!()
            );
        } else {
        }
    };

    ($e:expr, $($arg:tt)*) => {
        if let Err(err) = $e {
            log::debug!(
                "{:?}, {}, {}:{}:{}:{}",
                err,
                format_args!($($arg)*),
                module_path!(),
                file!(),
                line!(),
                column!()
            );
        } else {
        }
    };
}

#[inline]
pub fn timeout<T: std::future::Future>(ms: u64, future: T) -> tokio::time::Timeout<T> {
    tokio::time::timeout(std::time::Duration::from_millis(ms), future)
}

// 결과 타입 별칭 - 성공 또는 에러 반환
pub type ResultType<F, E = anyhow::Error> = anyhow::Result<F, E>;

/// 주소 변조(Address Mangling) 구조체
/// 일부 라우터와 방화벽은 NAT 매핑/변환에 사용하는 IP 주소 풀에 속하는 패킷을 스캔하므로,
/// 여기서 IP 주소를 변조하여 이러한 필터링을 우회합니다.
pub struct AddrMangle();

#[inline]
pub fn try_into_v4(addr: SocketAddr) -> SocketAddr {
    match addr {
        SocketAddr::V6(v6) if !addr.ip().is_loopback() => {
            if let Some(v4) = v6.ip().to_ipv4() {
                SocketAddr::new(IpAddr::V4(v4), addr.port())
            } else {
                addr
            }
        }
        _ => addr,
    }
}

impl AddrMangle {
    /// 소켓 주소를 바이트 배열로 인코딩합니다.
    /// [:1]:<port> 형식에서는 작동하지 않습니다.
    pub fn encode(addr: SocketAddr) -> Vec<u8> {
        let addr = try_into_v4(addr);
        match addr {
            SocketAddr::V4(addr_v4) => {
                let tm = (SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(std::time::Duration::ZERO)
                    .as_micros() as u32) as u128;
                let ip = u32::from_le_bytes(addr_v4.ip().octets()) as u128;
                let port = addr.port() as u128;
                let v = ((ip + tm) << 49) | (tm << 17) | (port + (tm & 0xFFFF));
                let bytes = v.to_le_bytes();
                let mut n_padding = 0;
                for i in bytes.iter().rev() {
                    if i == &0u8 {
                        n_padding += 1;
                    } else {
                        break;
                    }
                }
                bytes[..(16 - n_padding)].to_vec()
            }
            SocketAddr::V6(addr_v6) => {
                let mut x = addr_v6.ip().octets().to_vec();
                let port: [u8; 2] = addr_v6.port().to_le_bytes();
                x.push(port[0]);
                x.push(port[1]);
                x
            }
        }
    }

    /// 바이트 배열을 소켓 주소로 디코딩합니다.
    pub fn decode(bytes: &[u8]) -> SocketAddr {
        use std::convert::TryInto;

        if bytes.len() > 16 {
            if bytes.len() != 18 {
                return Config::get_any_listen_addr(false);
            }
            let tmp: [u8; 2] = bytes[16..].try_into().unwrap_or_default();
            let port = u16::from_le_bytes(tmp);
            let tmp: [u8; 16] = bytes[..16].try_into().unwrap_or_default();
            let ip = std::net::Ipv6Addr::from(tmp);
            return SocketAddr::new(IpAddr::V6(ip), port);
        }
        let mut padded = [0u8; 16];
        padded[..bytes.len()].copy_from_slice(bytes);
        let number = u128::from_le_bytes(padded);
        let tm = (number >> 17) & (u32::max_value() as u128);
        let ip = (((number >> 49) - tm) as u32).to_le_bytes();
        let port = (number & 0xFFFFFF) - (tm & 0xFFFF);
        SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]),
            port as u16,
        ))
    }
}

/// URL에서 버전 문자열을 추출합니다.
/// 예: "https://example.com/app-1.2.3" -> "-1.2.3"
pub fn get_version_from_url(url: &str) -> String {
    let n = url.chars().count();
    let a = url.chars().rev().position(|x| x == '-');
    if let Some(a) = a {
        let b = url.chars().rev().position(|x| x == '.');
        if let Some(b) = b {
            if a > b {
                if url
                    .chars()
                    .skip(n - b)
                    .collect::<String>()
                    .parse::<i32>()
                    .is_ok()
                {
                    return url.chars().skip(n - a).collect();
                } else {
                    return url.chars().skip(n - a).take(a - b - 1).collect();
                }
            } else {
                return url.chars().skip(n - a).collect();
            }
        }
    }
    "".to_owned()
}

/// 빌드 타임에 버전 정보를 생성합니다.
/// Cargo.toml에서 버전을 읽고 version.rs 파일을 생성합니다.
/// 빌드 날짜도 함께 기록됩니다.
pub fn gen_version() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    use std::io::prelude::*;
    let mut file = File::create("./src/version.rs").unwrap();
    for line in read_lines("Cargo.toml").unwrap().flatten() {
        let ab: Vec<&str> = line.split('=').map(|x| x.trim()).collect();
        if ab.len() == 2 && ab[0] == "version" {
            file.write_all(format!("pub const VERSION: &str = {};\n", ab[1]).as_bytes())
                .ok();
            break;
        }
    }
    // 빌드 날짜 생성
    let build_date = format!("{}", chrono::Local::now().format("%Y-%m-%d %H:%M"));
    file.write_all(
        format!("#[allow(dead_code)]\npub const BUILD_DATE: &str = \"{build_date}\";\n").as_bytes(),
    )
    .ok();
    file.sync_all().ok();
}

/// 파일의 각 라인을 반복 가능한 객체로 반환합니다.
fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

/// 커스텀 ID 유효성을 검증합니다.
/// 형식: 첫 글자는 알파벳, 이후 5~15자의 단어문자 또는 하이픈
/// 예: "device-123", "MyDeviceId"
pub fn is_valid_custom_id(id: &str) -> bool {
    regex::Regex::new(r"^[a-zA-Z][\w-]{5,15}$")
        .unwrap()
        .is_match(id)
}

/// 버전 문자열을 정수로 변환합니다.
/// 지원 형식: "1.1.10" 또는 "1.1.10-1" (- 뒤의 숫자는 패치 버전)
/// 변환 예: 1.1.10 -> 1001100, 1.2.3 -> 1002030
/// 패치 버전을 위해 마지막 번호에 10을 곱합니다.
pub fn get_version_number(v: &str) -> i64 {
    let mut versions = v.split('-');

    let mut n = 0;

    // 첫 번째 부분은 버전 번호입니다.
    // 1.1.10 -> 1001100, 1.2.3 -> 1002030
    // 패치 버전을 위해 마지막 숫자에 10을 곱합니다.
    if let Some(v) = versions.next() {
        let mut last = 0;
        for x in v.split('.') {
            last = x.parse::<i64>().unwrap_or(0);
            n = n * 1000 + last;
        }
        n -= last;
        n += last * 10;
    }

    // 패치 버전 추가
    if let Some(v) = versions.next() {
        n += v.parse::<i64>().unwrap_or(0);
    }

    n
}

/// 파일의 수정 시간을 반환합니다.
pub fn get_modified_time(path: &std::path::Path) -> SystemTime {
    std::fs::metadata(path)
        .map(|m| m.modified().unwrap_or(UNIX_EPOCH))
        .unwrap_or(UNIX_EPOCH)
}

/// 파일의 생성 시간을 반환합니다.
pub fn get_created_time(path: &std::path::Path) -> SystemTime {
    std::fs::metadata(path)
        .map(|m| m.created().unwrap_or(UNIX_EPOCH))
        .unwrap_or(UNIX_EPOCH)
}

/// 실행 파일의 수정 또는 생성 시간 중 더 최근의 시간을 반환합니다.
pub fn get_exe_time() -> SystemTime {
    std::env::current_exe().map_or(UNIX_EPOCH, |path| {
        let m = get_modified_time(&path);
        let c = get_created_time(&path);
        if m > c {
            m
        } else {
            c
        }
    })
}

/// 기기의 고유 UUID를 반환합니다.
/// machine_uid::get() 실패 사례:
/// - Windows 종료 시: "The media is write protected. (os error 19)"
/// - macOS (재현 어려움, 로그인 화면에서 재현됨): "No matching IOPlatformUUID in `ioreg -rd1 -c IOPlatformExpertDevice` command"
pub fn get_uuid() -> Vec<u8> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static CACHED_MACHINE_UID: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
        // Throttle only applies to the fallback machine_uid::get() log below, not the Once::call_once retry logs.
        static LOG_COUNT: AtomicUsize = AtomicUsize::new(0);

        // Only macOS needs retry logic here because:
        // - macOS: in testing, only one failure occurred when reading at 50ms intervals, so retry helps
        // - Windows: failures during shutdown are persistent, retrying is pointless
        #[cfg(target_os = "macos")]
        {
            static INIT: std::sync::Once = std::sync::Once::new();
            INIT.call_once(|| {
                // Keep in sync with upstream handling:
                // https://github.com/rustdesk/rustdesk/blob/85db6779828349b23ca3eba91cc7cd36c5337797/src/common.rs#L822
                let username = whoami::username().trim_end_matches('\0').to_owned();
                let max_retries = if username == "root" { 16 } else { 8 };
                for i in 0..max_retries {
                    match machine_uid::get() {
                        Ok(id) => {
                            let _ = CACHED_MACHINE_UID.set(id.into());
                            return;
                        }
                        Err(e) => {
                            log::error!("Failed to get machine uid in macOS retry #{i}: {e}");
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            });
        }

        if let Some(uid) = CACHED_MACHINE_UID.get() {
            return uid.clone();
        }

        match machine_uid::get() {
            Ok(id) => {
                let uid: Vec<u8> = id.into();
                let _ = CACHED_MACHINE_UID.set(uid.clone());
                return uid;
            }
            Err(e) => {
                if LOG_COUNT
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                        (count < 30).then_some(count + 1)
                    })
                    .is_ok()
                {
                    log::error!("Failed to get machine uid: {e}");
                }
            }
        }
    }
    Config::get_key_pair().1
}

#[inline]
pub fn get_time() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0) as _
}

#[inline]
pub fn is_ipv4_str(id: &str) -> bool {
    if let Ok(reg) = regex::Regex::new(
        r"^(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)(:\d+)?$",
    ) {
        reg.is_match(id)
    } else {
        false
    }
}

#[inline]
pub fn is_ipv6_str(id: &str) -> bool {
    if let Ok(reg) = regex::Regex::new(
        r"^((([a-fA-F0-9]{1,4}:{1,2})+[a-fA-F0-9]{1,4})|(\[([a-fA-F0-9]{1,4}:{1,2})+[a-fA-F0-9]{1,4}\]:\d+))$",
    ) {
        reg.is_match(id)
    } else {
        false
    }
}

#[inline]
pub fn is_ip_str(id: &str) -> bool {
    is_ipv4_str(id) || is_ipv6_str(id)
}

#[inline]
pub fn is_domain_port_str(id: &str) -> bool {
    // modified regex for RFC1123 hostname. check https://stackoverflow.com/a/106223 for original version for hostname.
    // according to [TLD List](https://data.iana.org/TLD/tlds-alpha-by-domain.txt) version 2023011700,
    // there is no digits in TLD, and length is 2~63.
    if let Ok(reg) = regex::Regex::new(
        r"(?i)^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z][a-z-]{0,61}[a-z]:\d{1,5}$",
    ) {
        reg.is_match(id)
    } else {
        false
    }
}

pub fn init_log(_is_async: bool, _name: &str) -> Option<flexi_logger::LoggerHandle> {
    static INIT: std::sync::Once = std::sync::Once::new();
    #[allow(unused_mut)]
    let mut logger_holder: Option<flexi_logger::LoggerHandle> = None;
    INIT.call_once(|| {
        #[cfg(debug_assertions)]
        {
            use env_logger::*;
            init_from_env(Env::default().filter_or(DEFAULT_FILTER_ENV, "info,reqwest=warn,rustls=warn,webrtc-sctp=warn,webrtc=warn"));
        }
        #[cfg(not(debug_assertions))]
        {
            // https://docs.rs/flexi_logger/latest/flexi_logger/error_info/index.html#write
            // though async logger more efficient, but it also causes more problems, disable it for now
            let mut path = config::Config::log_path();
            #[cfg(target_os = "android")]
            if !config::Config::get_home().exists() {
                return;
            }
            if !_name.is_empty() {
                path.push(_name);
            }
            use flexi_logger::*;
            if let Ok(x) = Logger::try_with_env_or_str("debug,reqwest=warn,rustls=warn,webrtc-sctp=warn,webrtc=warn") {
                logger_holder = x
                    .log_to_file(FileSpec::default().directory(path))
                    .write_mode(if _is_async {
                        WriteMode::Async
                    } else {
                        WriteMode::Direct
                    })
                    .format(opt_format)
                    .rotate(
                        Criterion::Age(Age::Day),
                        Naming::Timestamps,
                        Cleanup::KeepLogFiles(31),
                    )
                    .start()
                    .ok();
            }
        }
    });
    logger_holder
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct VersionCheckRequest {
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub os_version: String,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub device_id: Vec<u8>,
    #[serde(default)]
    pub typ: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct VersionCheckResponse {
    #[serde(default)]
    pub url: String,
}

pub const VER_TYPE_RUSTDESK_CLIENT: &str = "rustdesk-client";
pub const VER_TYPE_RUSTDESK_SERVER: &str = "rustdesk-server";

pub fn version_check_request(typ: String) -> (VersionCheckRequest, String) {
    const URL: &str = "https://api.rustdesk.com/version/latest";

    use sysinfo::System;
    let system = System::new();
    let os = system.distribution_id();
    let os_version = system.os_version().unwrap_or_default();
    let arch = std::env::consts::ARCH.to_string();
    #[allow(deprecated)]
    let device_id = fingerprint::get_fingerprint(None, None);
    (
        VersionCheckRequest {
            os,
            os_version,
            arch,
            device_id,
            typ,
        },
        URL.to_string(),
    )
}

pub fn time_based_rand() -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let mut x = nanos as u64;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;

    (x % 32768) as u32
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_mangle() {
        let addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 16, 32), 21116));
        assert_eq!(addr, AddrMangle::decode(&AddrMangle::encode(addr)));

        let addr = "[2001:db8::1]:8080".parse::<SocketAddr>().unwrap();
        assert_eq!(addr, AddrMangle::decode(&AddrMangle::encode(addr)));

        let addr = "[2001:db8:ff::1111]:80".parse::<SocketAddr>().unwrap();
        assert_eq!(addr, AddrMangle::decode(&AddrMangle::encode(addr)));
    }

    #[test]
    fn test_allow_err() {
        allow_err!(Err("test err") as Result<(), &str>);
        allow_err!(
            Err("test err with msg") as Result<(), &str>,
            "prompt {}",
            "failed"
        );
    }

    #[test]
    fn test_ipv6() {
        assert!(is_ipv6_str("1:2:3"));
        assert!(is_ipv6_str("[ab:2:3]:12"));
        assert!(is_ipv6_str("[ABEF:2a:3]:12"));
        assert!(!is_ipv6_str("[ABEG:2a:3]:12"));
        assert!(!is_ipv6_str("1[ab:2:3]:12"));
        assert!(!is_ipv6_str("1.1.1.1"));
        assert!(is_ip_str("1.1.1.1"));
        assert!(!is_ipv6_str("1:2:"));
        assert!(is_ipv6_str("1:2::0"));
        assert!(is_ipv6_str("[1:2::0]:1"));
        assert!(!is_ipv6_str("[1:2::0]:"));
        assert!(!is_ipv6_str("1:2::0]:1"));
    }

    #[test]
    fn test_ipv4() {
        assert!(is_ipv4_str("1.2.3.4"));
        assert!(is_ipv4_str("1.2.3.4:90"));
        assert!(is_ipv4_str("192.168.0.1"));
        assert!(is_ipv4_str("0.0.0.0"));
        assert!(is_ipv4_str("255.255.255.255"));
        assert!(!is_ipv4_str("256.0.0.0"));
        assert!(!is_ipv4_str("256.256.256.256"));
        assert!(!is_ipv4_str("1:2:"));
        assert!(!is_ipv4_str("192.168.0.256"));
        assert!(!is_ipv4_str("192.168.0.1/24"));
        assert!(!is_ipv4_str("192.168.0."));
        assert!(!is_ipv4_str("192.168..1"));
    }

    #[test]
    fn test_hostname_port() {
        assert!(!is_domain_port_str("a:12"));
        assert!(!is_domain_port_str("a.b.c:12"));
        assert!(is_domain_port_str("test.com:12"));
        assert!(is_domain_port_str("test-UPPER.com:12"));
        assert!(is_domain_port_str("some-other.domain.com:12"));
        assert!(!is_domain_port_str("under_score:12"));
        assert!(!is_domain_port_str("a@bc:12"));
        assert!(!is_domain_port_str("1.1.1.1:12"));
        assert!(!is_domain_port_str("1.2.3:12"));
        assert!(!is_domain_port_str("1.2.3.45:12"));
        assert!(!is_domain_port_str("a.b.c:123456"));
        assert!(!is_domain_port_str("---:12"));
        assert!(!is_domain_port_str(".:12"));
        // todo: should we also check for these edge cases?
        // out-of-range port
        assert!(is_domain_port_str("test.com:0"));
        assert!(is_domain_port_str("test.com:98989"));
    }

    #[test]
    fn test_mangle2() {
        let addr = "[::ffff:127.0.0.1]:8080".parse().unwrap();
        let addr_v4 = "127.0.0.1:8080".parse().unwrap();
        assert_eq!(AddrMangle::decode(&AddrMangle::encode(addr)), addr_v4);
        assert_eq!(
            AddrMangle::decode(&AddrMangle::encode("[::127.0.0.1]:8080".parse().unwrap())),
            addr_v4
        );
        assert_eq!(AddrMangle::decode(&AddrMangle::encode(addr_v4)), addr_v4);
        let addr_v6 = "[ef::fe]:8080".parse().unwrap();
        assert_eq!(AddrMangle::decode(&AddrMangle::encode(addr_v6)), addr_v6);
        let addr_v6 = "[::1]:8080".parse().unwrap();
        assert_eq!(AddrMangle::decode(&AddrMangle::encode(addr_v6)), addr_v6);
    }

    #[test]
    fn test_get_version_number() {
        assert_eq!(get_version_number("1.1.10"), 1001100);
        assert_eq!(get_version_number("1.1.10-1"), 1001101);
        assert_eq!(get_version_number("1.1.11-1"), 1001111);
        assert_eq!(get_version_number("1.2.3"), 1002030);
    }
}
