// 정규표현식 및 참조 해제 트레잇 임포트
use hbb_common::regex::Regex;
use std::ops::Deref;

// 아랍어 번역 모듈
mod ar;
// 지원되는 모든 언어 번역 모듈들
mod be;    // 벨라루스어
mod bg;    // 불가리아어
mod ca;    // 카탈로니아어
mod cn;    // 중국어 (간체)
mod cs;    // 체코어
mod da;    // 덴마크어
mod de;    // 독일어
mod el;    // 그리스어
mod en;    // 영어
mod eo;    // 에스페란토
mod es;    // 스페인어
mod et;    // 에스토니아어
mod eu;    // 바스크어
mod fa;    // 페르시아어
mod fr;    // 프랑스어
mod he;    // 히브리어
mod hr;    // 크로아티아어
mod hu;    // 헝가리어
mod id;    // 인도네시아어
mod it;    // 이탈리아어
mod ja;    // 일본어
mod ko;    // 한국어
mod kz;    // 카자흐어
mod lt;    // 리투아니아어
mod lv;    // 라트비아어
mod nb;    // 노르웨이 보크몰
mod nl;    // 네덜란드어
mod pl;    // 폴란드어
mod ptbr;  // 포르투갈어 (브라질)
mod ro;    // 루마니아어
mod ru;    // 러시아어
mod sc;    // 사르디아어
mod sk;    // 슬로바키아어
mod sl;    // 슬로베니아어
mod sq;    // 알바니아어
mod sr;    // 세르비아어
mod sv;    // 스웨덴어
mod th;    // 태국어
mod tr;    // 터키어
mod tw;    // 중국어 (번체)
mod uk;    // 우크라이나어
mod vi;    // 베트남어
mod ta;    // 타밀어
mod ge;    // 조지아어
mod fi;    // 핀란드어

pub const LANGS: &[(&str, &str)] = &[
    ("en", "English"),
    ("it", "Italiano"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("nl", "Nederlands"),
    ("nb", "Norsk bokmål"),
    ("zh-cn", "简体中文"),
    ("zh-tw", "繁體中文"),
    ("pt", "Português"),
    ("es", "Español"),
    ("et", "Eesti keel"),
    ("eu", "Euskara"),
    ("hu", "Magyar"),
    ("bg", "Български"),
    ("be", "Беларуская"),
    ("ru", "Русский"),
    ("sk", "Slovenčina"),
    ("id", "Indonesia"),
    ("cs", "Čeština"),
    ("da", "Dansk"),
    ("eo", "Esperanto"),
    ("tr", "Türkçe"),
    ("vi", "Tiếng Việt"),
    ("pl", "Polski"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("kz", "Қазақ"),
    ("uk", "Українська"),
    ("fa", "فارسی"),
    ("ca", "Català"),
    ("el", "Ελληνικά"),
    ("sv", "Svenska"),
    ("sq", "Shqip"),
    ("sr", "Srpski"),
    ("th", "ภาษาไทย"),
    ("sl", "Slovenščina"),
    ("ro", "Română"),
    ("lt", "Lietuvių"),
    ("lv", "Latviešu"),
    ("ar", "العربية"),
    ("he", "עברית"),
    ("hr", "Hrvatski"),
    ("sc", "Sardu"),
    ("ta", "தமிழ்"),
    ("ge", "ქართული"),
    ("fi", "Suomi"),
];

/// 시스템 로케일에 따라 문자열을 번역한다.
/// 안드로이드 및 iOS에서는 사용 불가능하다.
///
/// # 인자
/// * `name` - 번역할 원본 문자열
///
/// # 반환
/// 현재 시스템 로케일에 맞는 번역된 문자열
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn translate(name: String) -> String {
    let locale = sys_locale::get_locale().unwrap_or_default();
    translate_locale(name, &locale)
}

/// 주어진 로케일에 따라 문자열을 번역한다.
/// 사용자 설정 언어, 시스템 로케일, 기본값 순으로 우선순위를 결정한다.
///
/// # 인자
/// * `name` - 번역할 원본 문자열 (키 역할)
/// * `locale` - 로케일 문자열 (예: "zh_CN", "en_US")
///
/// # 반환
/// 번역된 문자열, 해당 언어 번역이 없으면 원본 문자열 반환
pub fn translate_locale(name: String, locale: &str) -> String {
    let locale = locale.to_lowercase();
    // 먼저 사용자 설정 언어 확인
    let mut lang = hbb_common::config::LocalConfig::get_option("lang").to_lowercase();
    if lang.is_empty() {
        // 중국어 특별 처리 (Linux: zh_CN, macOS: zh-Hans-CN, Android: zh_CN_#Hans)
        if locale.starts_with("zh") {
            lang = (if locale.contains("tw") {
                "zh-tw"
            } else {
                "zh-cn"
            })
            .to_owned();
        }
    }
    if lang.is_empty() {
        // 로케일에서 언어 코드 추출 (예: "en_US" -> "en")
        lang = locale
            .split("-")
            .next()
            .map(|x| x.split("_").next().unwrap_or_default())
            .unwrap_or_default()
            .to_owned();
    }
    // 언어 코드를 소문자로 변환하고 해당 언어 번역 맵 선택
    let lang = lang.to_lowercase();
    let m = match lang.as_str() {
        "fr" => fr::T.deref(),       // 프랑스어
        "zh-cn" => cn::T.deref(),    // 중국어 (간체)
        "it" => it::T.deref(),       // 이탈리아어
        "zh-tw" => tw::T.deref(),    // 중국어 (번체)
        "de" => de::T.deref(),       // 독일어
        "nb" => nb::T.deref(),       // 노르웨이어
        "nl" => nl::T.deref(),       // 네덜란드어
        "es" => es::T.deref(),       // 스페인어
        "et" => et::T.deref(),       // 에스토니아어
        "eu" => eu::T.deref(),       // 바스크어
        "hu" => hu::T.deref(),       // 헝가리어
        "ru" => ru::T.deref(),       // 러시아어
        "eo" => eo::T.deref(),       // 에스페란토
        "id" => id::T.deref(),       // 인도네시아어
        "br" => ptbr::T.deref(),     // 포르투갈어 (브라질)
        "pt" => ptbr::T.deref(),     // 포르투갈어 (포르투갈)
        "tr" => tr::T.deref(),       // 터키어
        "cs" => cs::T.deref(),       // 체코어
        "da" => da::T.deref(),       // 덴마크어
        "sk" => sk::T.deref(),       // 슬로바키아어
        "vi" => vi::T.deref(),       // 베트남어
        "pl" => pl::T.deref(),       // 폴란드어
        "ja" => ja::T.deref(),       // 일본어
        "ko" => ko::T.deref(),       // 한국어
        "kz" => kz::T.deref(),       // 카자흐어
        "uk" => uk::T.deref(),       // 우크라이나어
        "fa" => fa::T.deref(),       // 페르시아어
        "fi" => fi::T.deref(),       // 핀란드어
        "ca" => ca::T.deref(),       // 카탈로니아어
        "el" => el::T.deref(),       // 그리스어
        "sv" => sv::T.deref(),       // 스웨덴어
        "sq" => sq::T.deref(),       // 알바니아어
        "sr" => sr::T.deref(),       // 세르비아어
        "th" => th::T.deref(),       // 태국어
        "sl" => sl::T.deref(),       // 슬로베니아어
        "ro" => ro::T.deref(),       // 루마니아어
        "lt" => lt::T.deref(),       // 리투아니아어
        "lv" => lv::T.deref(),       // 라트비아어
        "ar" => ar::T.deref(),       // 아랍어
        "bg" => bg::T.deref(),       // 불가리아어
        "be" => be::T.deref(),       // 벨라루스어
        "he" => he::T.deref(),       // 히브리어
        "hr" => hr::T.deref(),       // 크로아티아어
        "sc" => sc::T.deref(),       // 사르디아어
        "ta" => ta::T.deref(),       // 타밀어
        "ge" => ge::T.deref(),       // 조지아어
        _ => en::T.deref(),          // 기본값: 영어
    };
    // 번역 문자열에서 플레이스홀더 추출 (예: "There are {24} hours")
    let (name, placeholder_value) = extract_placeholder(&name);

    // 번역된 문자열에 플레이스홀더와 앱명을 대체하는 클로저
    let replace = |s: &&str| {
        let mut s = s.to_string();
        // 플레이스홀더 값이 있으면 {} 대체
        if let Some(value) = placeholder_value.as_ref() {
            s = s.replace("{}", &value);
        }
        // RustDesk가 아닌 커스텀 애플리케이션일 경우 앱명으로 변경
        if !crate::is_rustdesk() {
            if s.contains("RustDesk")
                && !name.starts_with("upgrade_rustdesk_server_pro")
                && name != "powered_by_me"
            {
                let app_name = crate::get_app_name();
                if !app_name.contains("RustDesk") {
                    // 단순 교체
                    s = s.replace("RustDesk", &app_name);
                } else {
                    // RustDesk 문자열이 포함된 앱명(예: "RustDesk-Admin")의 경우
                    // 무한 반복 교체를 피하기 위해 임시 플레이스홀더 사용
                    // https://github.com/rustdesk/rustdesk-server-pro/issues/845
                    const PLACEHOLDER: &str = "#A-P-P-N-A-M-E#";
                    if !s.contains(PLACEHOLDER) {
                        s = s.replace(&app_name, PLACEHOLDER);
                        s = s.replace("RustDesk", &app_name);
                        s = s.replace(PLACEHOLDER, &app_name);
                    } else {
                        // 플레이스홀더가 이미 있는 경우는 매우 드물므로 교체 건너뜀
                    }
                }
            }
        }
        s
    };
    // 선택된 언어의 번역 찾기
    if let Some(v) = m.get(&name as &str) {
        if !v.is_empty() {
            return replace(v);
        }
    }
    // 선택된 언어가 영어가 아니면 영어 번역 시도
    if lang != "en" {
        if let Some(v) = en::T.get(&name as &str) {
            if !v.is_empty() {
                return replace(v);
            }
        }
    }
    // 번역이 없으면 원본 키 반환
    replace(&name.as_str())
}

/// 입력 문자열에서 플레이스홀더를 추출하는 함수
/// 플레이스홀더 패턴: {값}
///
/// # 사용 방법
/// UI에는 {값}으로 작성: translate("There are {24} hours in a day")
/// 번역 파일에는 {}으로 작성: ("There are {} hours in a day", ...)
///
/// # 반환값
/// (정규화된_문자열, 추출된_값)
/// 예: "There are {24} hours" -> ("There are {} hours", Some("24"))
fn extract_placeholder(input: &str) -> (String, Option<String>) {
    if let Ok(re) = Regex::new(r#"\{(.*?)\}"#) {
        if let Some(captures) = re.captures(input) {
            if let Some(inner_match) = captures.get(1) {
                let name = re.replace(input, "{}").to_string();
                let value = inner_match.as_str().to_string();
                return (name, Some(value));
            }
        }
    }
    (input.to_string(), None)
}

// 플레이스홀더 추출 함수 테스트
mod test {
    #[test]
    fn test_extract_placeholders() {
        use super::extract_placeholder as f;

        // 빈 문자열 테스트
        assert_eq!(f(""), ("".to_string(), None));
        // 숫자 플레이스홀더 테스트
        assert_eq!(
            f("{3} sessions"),
            ("{} sessions".to_string(), Some("3".to_string()))
        );
        // 잘못된 패턴 테스트 (닫는 괄호 먼저 나오는 경우)
        assert_eq!(f(" } { "), (" } { ".to_string(), None));
        // 빈 플레이스홀더 테스트
        assert_eq!(
            f("{} sessions"),
            ("{} sessions".to_string(), Some("".to_string()))
        );
        // 첫 번째 플레이스홀더만 추출 (여러 개가 있을 때)
        assert_eq!(
            f("{2} times {4} makes {8}"),
            ("{} times {4} makes {8}".to_string(), Some("2".to_string()))
        );
    }
}
