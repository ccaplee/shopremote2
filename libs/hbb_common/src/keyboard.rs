use std::{fmt, slice::Iter, str::FromStr};

use crate::protos::message::KeyboardMode;

/// KeyboardMode를 문자열로 변환합니다.
/// Legacy (레거시 모드), Map (매핑 모드), Translate (번역 모드), Auto (자동 선택)
impl fmt::Display for KeyboardMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            KeyboardMode::Legacy => write!(f, "legacy"),      // 레거시 키보드 처리
            KeyboardMode::Map => write!(f, "map"),            // 키맵 기반 처리
            KeyboardMode::Translate => write!(f, "translate"), // 키 번역 기반 처리
            KeyboardMode::Auto => write!(f, "auto"),          // 자동으로 최적 모드 선택
        }
    }
}

/// 문자열을 KeyboardMode로 파싱합니다.
impl FromStr for KeyboardMode {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "legacy" => Ok(KeyboardMode::Legacy),
            "map" => Ok(KeyboardMode::Map),
            "translate" => Ok(KeyboardMode::Translate),
            "auto" => Ok(KeyboardMode::Auto),
            _ => Err(()),
        }
    }
}

impl KeyboardMode {
    /// 모든 KeyboardMode 값을 반복 가능한 컬렉션으로 반환합니다.
    pub fn iter() -> Iter<'static, KeyboardMode> {
        static KEYBOARD_MODES: [KeyboardMode; 4] = [
            KeyboardMode::Legacy,
            KeyboardMode::Map,
            KeyboardMode::Translate,
            KeyboardMode::Auto,
        ];
        KEYBOARD_MODES.iter()
    }
}
