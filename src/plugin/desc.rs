use hbb_common::ResultType;
use serde_derive::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::ffi::{c_char, CStr};

/// UI 버튼 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiButton {
    /// 버튼의 고유 키
    key: String,
    /// 버튼에 표시될 텍스트
    text: String,
    /// 버튼 아이콘 (Flutter에서는 정수이지만, 다른 UI 프레임워크를 지원하기 위해 문자열로 통일)
    icon: String,
    /// 버튼의 툴팁 텍스트
    tooltip: String,
    /// 버튼 클릭 시 실행할 작업
    action: String,
}

/// UI 체크박스 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiCheckbox {
    /// 체크박스의 고유 키
    key: String,
    /// 체크박스에 표시될 텍스트
    text: String,
    /// 체크박스의 툴팁 텍스트
    tooltip: String,
    /// 체크박스 상태 변경 시 실행할 작업
    action: String,
}

/// UI 요소의 종류를 정의하는 열거형 (버튼 또는 체크박스)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum UiType {
    /// 버튼 UI 요소
    Button(UiButton),
    /// 체크박스 UI 요소
    Checkbox(UiCheckbox),
}

/// UI 요소의 위치별 그룹을 나타내는 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    /// 위치별 UI 요소 목록 (위치 문자열 -> UI 요소 벡터)
    pub ui: HashMap<String, Vec<UiType>>,
}

/// 플러그인의 설정 항목을 정의하는 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigItem {
    /// 설정의 고유 키
    pub key: String,
    /// 설정의 기본값
    pub default: String,
    /// 설정에 대한 설명
    pub description: String,
}

/// 플러그인의 모든 설정을 정의하는 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 공유 설정 항목 목록
    pub shared: Vec<ConfigItem>,
    /// 피어별 설정 항목 목록
    pub peer: Vec<ConfigItem>,
}

/// 플러그인의 출판 정보를 저장하는 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishInfo {
    /// 출판 날짜
    pub published: String,
    /// 마지막 출시 날짜
    pub last_released: String,
}

/// 플러그인의 메타정보를 정의하는 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    /// 플러그인의 고유 ID
    pub id: String,
    /// 플러그인 이름
    pub name: String,
    /// 플러그인 버전
    pub version: String,
    /// 플러그인 설명
    pub description: String,
    /// 지원하는 플랫폼 (예: "windows,linux,macos")
    #[serde(default)]
    pub platforms: String,
    /// 플러그인 개발자
    pub author: String,
    /// 플러그인 홈페이지 URL
    pub home: String,
    /// 플러그인 라이선스
    pub license: String,
    /// 플러그인 소스 코드 URL
    pub source: String,
    /// 플러그인 출판 정보
    pub publish_info: PublishInfo,
}

/// 플러그인의 전체 설명을 정의하는 구조체
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Desc {
    /// 플러그인 메타정보
    meta: Meta,
    /// 플러그인 활성화 시 재부팅 필요 여부
    need_reboot: bool,
    /// 플러그인 UI 위치 정보
    location: Location,
    /// 플러그인 설정 정보
    config: Config,
    /// 플러그인이 리스닝할 이벤트 목록
    listen_events: Vec<String>,
}

impl Desc {
    /// C 문자열에서 Desc 구조체를 파싱합니다
    pub fn from_cstr(s: *const c_char) -> ResultType<Self> {
        let s = unsafe { CStr::from_ptr(s) };
        Ok(serde_json::from_str(s.to_str()?)?)
    }

    /// 플러그인 메타정보를 반환합니다
    pub fn meta(&self) -> &Meta {
        &self.meta
    }

    /// 플러그인 UI 위치 정보를 반환합니다
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// 플러그인 설정 정보를 반환합니다
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// 플러그인이 리스닝할 이벤트 목록을 반환합니다
    pub fn listen_events(&self) -> &Vec<String> {
        &self.listen_events
    }
}
