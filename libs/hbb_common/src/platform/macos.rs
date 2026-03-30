use crate::ResultType;
use osascript;
use serde_derive::{Deserialize, Serialize};

/// 알림 다이얼로그의 매개변수
#[derive(Serialize)]
struct AlertParams {
    title: String,           // 알림 제목
    message: String,         // 알림 메시지
    alert_type: String,      // informational, warning, critical
    buttons: Vec<String>,    // 버튼 목록
}

/// 알림 다이얼로그의 결과
#[derive(Deserialize)]
struct AlertResult {
    #[serde(rename = "buttonReturned")]
    button: String,          // 클릭된 버튼의 이름
}

/// macOS 네이티브 알림 다이얼로그를 표시합니다.
/// 지정된 앱에서 스크립트를 실행한 후 알림 다이얼로그를 표시하고,
/// 사용자가 클릭한 버튼의 값을 반환합니다.
///
/// # 인수
///
/// * `app` - 스크립트를 실행할 앱 (e.g., "Finder", "System Events")
/// * `alert_type` - 알림 타입: informational, warning, critical
/// * `title` - 알림 제목
/// * `message` - 알림 메시지
/// * `buttons` - 표시할 버튼 목록
pub fn alert(
    app: String,
    alert_type: String,
    title: String,
    message: String,
    buttons: Vec<String>,
) -> ResultType<String> {
    let script = osascript::JavaScript::new(&format!(
        "
    var App = Application('{}');
    App.includeStandardAdditions = true;
    return App.displayAlert($params.title, {{
        message: $params.message,
        'as': $params.alert_type,
        buttons: $params.buttons,
    }});
    ",
        app
    ));

    let result: AlertResult = script.execute_with_params(AlertParams {
        title,
        message,
        alert_type,
        buttons,
    })?;
    Ok(result.button)
}
