use hbb_common::{
    get_time,
    message_proto::{Message, VoiceCallRequest, VoiceCallResponse},
};
use scrap::CodecFormat;
use std::collections::HashMap;

/// 비디오 스트림의 품질 상태를 나타내는 구조체
/// 네트워크 대역폭, 프레임 속도, 지연시간, 비트레이트 등의 정보를 추적
#[derive(Debug, Default)]
pub struct QualityStatus {
    /// 현재 네트워크 속도 (선택사항)
    pub speed: Option<String>,
    /// 각 스트림별 초당 프레임 수 (FPS) 정보
    pub fps: HashMap<usize, i32>,
    /// 네트워크 지연시간 (밀리초 단위, 선택사항)
    pub delay: Option<i32>,
    /// 목표 비트레이트 (bps 단위, 선택사항)
    pub target_bitrate: Option<i32>,
    /// 비디오 코덱 형식 (선택사항)
    pub codec_format: Option<CodecFormat>,
    /// 색상 공간 정보 (선택사항)
    pub chroma: Option<String>,
}

/// 음성 통화 연결 요청 메시지 생성
/// is_connect: true면 연결 요청, false면 연결 해제 요청
/// 반환값: 프로토콜 메시지로 직렬화 가능한 VoiceCallRequest 포함 메시지
#[inline]
pub fn new_voice_call_request(is_connect: bool) -> Message {
    let mut req = VoiceCallRequest::new();
    req.is_connect = is_connect;
    req.req_timestamp = get_time();
    let mut msg = Message::new();
    msg.set_voice_call_request(req);
    msg
}

/// 음성 통화 연결 요청에 대한 응답 메시지 생성
/// request_timestamp: 원본 요청의 타임스탬프
/// accepted: 연결 요청 수락 여부
/// 반환값: 프로토콜 메시지로 직렬화 가능한 VoiceCallResponse 포함 메시지
#[inline]
pub fn new_voice_call_response(request_timestamp: i64, accepted: bool) -> Message {
    let mut resp = VoiceCallResponse::new();
    resp.accepted = accepted;
    resp.req_timestamp = request_timestamp;
    resp.ack_timestamp = get_time();
    let mut msg = Message::new();
    msg.set_voice_call_response(resp);
    msg
}
