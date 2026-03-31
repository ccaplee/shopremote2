#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use libshopremote2::*;

/// Android 및 iOS 플랫폼이나 Flutter 기능이 활성화되었을 때의 메인 함수
/// 전역 초기화, 랑데뷰 서버 테스트, NAT 타입 테스트를 수행한 후 전역 정리를 진행한다.
#[cfg(any(target_os = "android", target_os = "ios", feature = "flutter"))]
fn main() {
    // 전역 초기화 수행
    if !common::global_init() {
        eprintln!("Global initialization failed.");
        return;
    }
    // 랑데뷰 서버 연결 테스트
    common::test_rendezvous_server();
    // NAT 타입 감지 테스트
    common::test_nat_type();
    // 전역 리소스 정리
    common::global_clean();
}

/// 데스크톱 플랫폼(Windows, macOS, Linux)에서의 메인 함수
/// 고DPI 인식 설정, UI 시작, 전역 정리를 수행한다.
#[cfg(not(any(
    target_os = "android",
    target_os = "ios",
    feature = "cli",
    feature = "flutter"
)))]
fn main() {
    // Windows에서 고DPI 인식 활성화 - UI 스케일링 문제 해결
    #[cfg(all(windows, not(feature = "inline")))]
    unsafe {
        winapi::um::shellscalingapi::SetProcessDpiAwareness(2);
    }
    // 핵심 초기화 및 인자 처리
    if let Some(args) = crate::core_main::core_main().as_mut() {
        // UI 시작
        ui::start(args);
    }
    // 종료 전 리소스 정리
    common::global_clean();
}

/// CLI(명령줄 인터페이스) 모드의 메인 함수
/// 포트 포워딩, 원격 연결 테스트, 서버 시작 등을 명령줄로 수행한다.
#[cfg(feature = "cli")]
fn main() {
    // 전역 초기화
    if !common::global_init() {
        return;
    }
    use clap::App;
    use hbb_common::log;
    // 명령줄 인자 정의
    let args = format!(
        "-p, --port-forward=[PORT-FORWARD-OPTIONS] 'Format: remote-id:local-port:remote-port[:remote-host]'
        -c, --connect=[REMOTE_ID] 'test only'
        -k, --key=[KEY] ''
       -s, --server=[] 'Start server'",
    );
    // 명령줄 파서 설정 및 인자 파싱
    let matches = App::new("shopremote2")
        .version(crate::VERSION)
        .author("ShopRemote2<ccccap@naver.com>")
        .about("ShopRemote2 command line tool")
        .args_from_usage(&args)
        .get_matches();
    use hbb_common::{config::LocalConfig, env_logger::*};
    // 로깅 초기화
    init_from_env(Env::default().filter_or(DEFAULT_FILTER_ENV, "info"));
    // 포트 포워딩 옵션 처리
    if let Some(p) = matches.value_of("port-forward") {
        // 포트 포워딩 옵션을 colon으로 분리
        let options: Vec<String> = p.split(":").map(|x| x.to_owned()).collect();
        // 최소 3개 항목 필요 (remote-id:local-port:remote-port)
        if options.len() < 3 {
            log::error!("Wrong port-forward options");
            return;
        }
        // 로컬 포트 파싱
        let mut port = 0;
        if let Ok(v) = options[1].parse::<i32>() {
            port = v;
        } else {
            log::error!("Wrong local-port");
            return;
        }
        // 원격 포트 파싱
        let mut remote_port = 0;
        if let Ok(v) = options[2].parse::<i32>() {
            remote_port = v;
        } else {
            log::error!("Wrong remote-port");
            return;
        }
        // 원격 호스트 설정 (기본값: localhost)
        let mut remote_host = "localhost".to_owned();
        if options.len() > 3 {
            remote_host = options[3].clone();
        }
        // 서버 연결성 테스트
        common::test_rendezvous_server();
        common::test_nat_type();
        // 접근 토큰 및 키 가져오기
        let key = matches.value_of("key").unwrap_or("").to_owned();
        let token = LocalConfig::get_option("access_token");
        // 포트 포워딩 시작
        cli::start_one_port_forward(
            options[0].clone(),
            port,
            remote_host,
            remote_port,
            key,
            token,
        );
    } else if let Some(p) = matches.value_of("connect") {
        // 원격 연결 테스트 모드
        common::test_rendezvous_server();
        common::test_nat_type();
        let key = matches.value_of("key").unwrap_or("").to_owned();
        let token = LocalConfig::get_option("access_token");
        cli::connect_test(p, key, token);
    } else if let Some(p) = matches.value_of("server") {
        // 서버 모드
        log::info!("id={}", hbb_common::config::Config::get_id());
        crate::start_server(true, false);
    }
    // 종료 전 전역 정리
    common::global_clean();
}
