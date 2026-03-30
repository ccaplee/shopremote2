// 콜백을 위한 외부 지원.
// 1. 일부 플러그인을 위한 입력 차단 지원.
// -----------------------------------------------------------------------------

use super::*;

const EXT_SUPPORT_BLOCK_INPUT: &str = "block-input";

/// 외부 지원 콜백을 처리합니다.
///
/// 이 함수는 플러그인으로부터 받은 외부 지원 메시지를 처리하고,
/// 입력 차단 등의 기능을 수행합니다.
///
/// # 매개변수
/// * `id` - 플러그인 ID
/// * `peer` - 피어 ID
/// * `msg` - 외부 지원 메시지
///
/// # 반환값
/// 작업 결과를 포함한 PluginReturn
pub(super) fn ext_support_callback(
    id: &str,
    peer: &str,
    msg: &super::callback_msg::MsgToExtSupport,
) -> PluginReturn {
    match &msg.r#type as _ {
        EXT_SUPPORT_BLOCK_INPUT => {
            // 지원되는 플러그인 목록을 확인할 수 있습니다.
            // let supported_plugins = [];
            // let supported = supported_plugins.contains(&id);
            let supported = true;
            if supported {
                // 데이터 길이 검증
                if msg.data.len() != 1 {
                    return PluginReturn::new(
                        errno::ERR_CALLBACK_INVALID_ARGS,
                        "Invalid data length",
                    );
                }
                // 입력 차단 여부를 데이터에서 추출
                let block = msg.data[0] != 0;
                if crate::server::plugin_block_input(peer, block) == block {
                    PluginReturn::success()
                } else {
                    PluginReturn::new(errno::ERR_CALLBACK_FAILED, "")
                }
            } else {
                // 지원되지 않는 플러그인인 경우
                PluginReturn::new(
                    errno::ERR_CALLBACK_PLUGIN_ID,
                    &format!("This operation is not supported for plugin '{}', please contact the RustDesk team for support.", id),
                )
            }
        }
        _ => PluginReturn::new(
            errno::ERR_CALLBACK_TARGET_TYPE,
            &format!("Unknown target type '{}'", &msg.r#type),
        ),
    }
}
