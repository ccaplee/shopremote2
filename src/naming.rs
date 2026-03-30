// 커스텀 서버 설정 모듈
mod custom_server;
use hbb_common::{ResultType, base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _}};
use custom_server::*;

/// CustomServer 구조체를 Base64로 인코딩하고 역순으로 정렬하여
/// 실행 파일명으로 사용할 문자열을 생성하는 함수
///
/// # 인자
/// * `lic` - 커스텀 서버 설정 구조체
///
/// # 반환
/// 결과: 파일명용 인코딩된 문자열
fn gen_name(lic: &CustomServer) -> ResultType<String> {
    // CustomServer를 JSON으로 직렬화한 후 Base64로 인코딩
    let tmp = URL_SAFE_NO_PAD.encode(&serde_json::to_vec(lic)?);
    // 인코딩된 문자열을 역순으로 정렬하여 반환
    Ok(tmp.chars().rev().collect())
}

/// 커스텀 서버 설정을 인코딩/디코딩하는 명명 도구
///
/// 사용 방법:
/// 1. 인코딩: ./naming key host [api] [relay]
///    -> 출력: rustdesk-custom_serverd-{인코딩된_문자열}.exe
/// 2. 디코딩: ./naming {인코딩된_문자열}
///    -> 출력: 원본 CustomServer 구조체
fn main() {
    // 첫 번째 인자 이후를 모두 가져옴
    let args: Vec<_> = std::env::args().skip(1).collect();
    // 선택적 인자들
    let api = args.get(2).cloned().unwrap_or_default();
    let relay = args.get(3).cloned().unwrap_or_default();

    // 2개 이상의 인자가 있으면 인코딩 모드
    if args.len() >= 2 {
        match gen_name(&CustomServer {
            key: args[0].clone(),
            host: args[1].clone(),
            api,
            relay,
        }) {
            Ok(name) => println!("rustdesk-custom_serverd-{}.exe", name),
            Err(e) => println!("{:?}", e),
        }
    }
    // 정확히 1개 인자만 있으면 디코딩 모드
    if args.len() == 1 {
        println!("{:?}", get_custom_server_from_string(&args[0]));
    }
}
