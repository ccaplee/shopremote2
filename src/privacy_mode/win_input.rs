use hbb_common::{allow_err, bail, lazy_static, log, ResultType};
use std::{
    io::Error,
    sync::{
        mpsc::{channel, Sender},
        Mutex,
    },
};
use winapi::{
    ctypes::c_int,
    shared::{
        minwindef::{DWORD, FALSE, HMODULE, LOBYTE, LPARAM, LRESULT, UINT, WPARAM},
        ntdef::NULL,
        windef::{HHOOK, POINT},
    },
    um::{libloaderapi::GetModuleHandleExA, processthreadsapi::GetCurrentThreadId, winuser::*},
};

// GetModuleHandleEx API 플래그: 참조 카운트를 증가시키지 않음
const GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT: u32 = 2;
// GetModuleHandleEx API 플래그: 주소로부터 모듈 핸들을 가져옴
const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: u32 = 4;

// 프라이버시 모드 훅 종료 신호를 위한 사용자 정의 윈도우 메시지
const WM_USER_EXIT_HOOK: u32 = WM_USER + 1;

// 현재 활성 훅이 설치된 스레드의 ID를 저장하는 전역 뮤텍스
// 0이면 훅이 설치되지 않음, 0이 아니면 해당 스레드 ID에 훅이 설치됨
lazy_static::lazy_static! {
    static ref CUR_HOOK_THREAD_ID: Mutex<DWORD> = Mutex::new(0);
}

/// 키보드와 마우스 입력 훅을 설치하는 내부 함수입니다.
///
/// 프라이버시 모드에서 사용자의 입력을 모니터링하고 제어하기 위해
/// 저수준 키보드(WH_KEYBOARD_LL)와 마우스(WH_MOUSE_LL) 훅을 Windows 시스템에 설치합니다.
/// 훅이 이미 설치되어 있으면 중복 설치를 방지합니다.
///
/// # 인자
/// - `tx`: 훅 설치 결과 메시지를 전달할 채널 송신자
///
/// # 반환값
/// - Ok((keyboard_hook, mouse_hook)): 성공 시 설치된 키보드 및 마우스 훅 핸들
/// - Ok((0, 0)): 실패 시 null 핸들 반환 (오류 메시지는 tx를 통해 전달됨)
/// - Err: 채널 전송 실패 등의 예외 상황
fn do_hook(tx: Sender<String>) -> ResultType<(HHOOK, HHOOK)> {
    let invalid_ret = (0 as HHOOK, 0 as HHOOK);

    let mut cur_hook_thread_id = CUR_HOOK_THREAD_ID.lock().unwrap();
    // 훅이 이미 설치되었으면 중복 설치 방지
    if *cur_hook_thread_id != 0 {
        tx.send("Already hooked".to_owned())?;
        return Ok(invalid_ret);
    }

    unsafe {
        // 키보드 훅을 위한 모듈 핸들 획득
        let mut hm_keyboard = 0 as HMODULE;
        if 0 == GetModuleHandleExA(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            DefWindowProcA as _,
            &mut hm_keyboard as _,
        ) {
            tx.send(format!(
                "Failed to GetModuleHandleExA, error: {}",
                Error::last_os_error()
            ))?;
            return Ok(invalid_ret);
        }
        // 마우스 훅을 위한 모듈 핸들 획득
        let mut hm_mouse = 0 as HMODULE;
        if 0 == GetModuleHandleExA(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            DefWindowProcA as _,
            &mut hm_mouse as _,
        ) {
            tx.send(format!(
                "Failed to GetModuleHandleExA, error: {}",
                Error::last_os_error()
            ))?;
            return Ok(invalid_ret);
        }

        // 저수준 키보드 훅 설치
        let hook_keyboard = SetWindowsHookExA(
            WH_KEYBOARD_LL,
            Some(privacy_mode_hook_keyboard),
            hm_keyboard,
            0,
        );
        if hook_keyboard.is_null() {
            tx.send(format!(
                "SetWindowsHookExA keyboard, error {}",
                Error::last_os_error()
            ))?;
            return Ok(invalid_ret);
        }

        // 저수준 마우스 훅 설치
        let hook_mouse = SetWindowsHookExA(WH_MOUSE_LL, Some(privacy_mode_hook_mouse), hm_mouse, 0);
        if hook_mouse.is_null() {
            // 키보드 훅 설치에는 성공했지만 마우스 훅 설치 실패 시 키보드 훅 제거
            if FALSE == UnhookWindowsHookEx(hook_keyboard) {
                log::error!(
                    "UnhookWindowsHookEx keyboard, error {}",
                    Error::last_os_error()
                );
            }
            tx.send(format!(
                "SetWindowsHookExA mouse, error {}",
                Error::last_os_error()
            ))?;
            return Ok(invalid_ret);
        }

        // 현재 스레드 ID를 저장하여 훅이 설치된 상태 표시
        *cur_hook_thread_id = GetCurrentThreadId();
        tx.send("".to_owned())?;
        return Ok((hook_keyboard, hook_mouse));
    }
}

/// 프라이버시 모드 입력 훅을 설치하고 활성화합니다.
///
/// 새로운 스레드에서 키보드 및 마우스 입력 훅을 설치하고,
/// 해당 스레드의 메시지 루프를 시작합니다.
/// 훅이 해제될 때까지 메시지 루프는 계속 실행되며,
/// 사용자 입력 이벤트를 감시하고 필요에 따라 필터링합니다.
///
/// # 반환값
/// - Ok(()): 훅 설치 성공
/// - Err: 훅 설치 실패 또는 메시지 채널 오류
pub fn hook() -> ResultType<()> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let hook_keyboard;
        let hook_mouse;
        unsafe {
            // 키보드 및 마우스 훅 설치
            match do_hook(tx.clone()) {
                Ok(hooks) => {
                    hook_keyboard = hooks.0;
                    hook_mouse = hooks.1;
                }
                Err(e) => {
                    allow_err!(tx.send(format!("Unexpected err when hook {}", e)));
                    return;
                }
            }
            if hook_keyboard.is_null() {
                return;
            }

            // 메시지 루프 초기화
            let mut msg = MSG {
                hwnd: NULL as _,
                message: 0 as _,
                wParam: 0 as _,
                lParam: 0 as _,
                time: 0 as _,
                pt: POINT {
                    x: 0 as _,
                    y: 0 as _,
                },
            };
            // 훅 종료 신호(WM_USER_EXIT_HOOK)를 받을 때까지 메시지 루프 실행
            while FALSE != GetMessageA(&mut msg, NULL as _, 0, 0) {
                if msg.message == WM_USER_EXIT_HOOK {
                    break;
                }

                TranslateMessage(&msg);
                DispatchMessageA(&msg);
            }

            // 키보드 훅 제거
            if FALSE == UnhookWindowsHookEx(hook_keyboard as _) {
                log::error!(
                    "Failed UnhookWindowsHookEx keyboard, error {}",
                    Error::last_os_error()
                );
            }

            // 마우스 훅 제거
            if FALSE == UnhookWindowsHookEx(hook_mouse as _) {
                log::error!(
                    "Failed UnhookWindowsHookEx mouse, error {}",
                    Error::last_os_error()
                );
            }

            // 훅 스레드 ID 초기화
            *CUR_HOOK_THREAD_ID.lock().unwrap() = 0;
        }
    });

    // 훅 설치 결과 대기
    match rx.recv() {
        Ok(msg) => {
            if msg == "" {
                Ok(())
            } else {
                bail!(msg)
            }
        }
        Err(e) => {
            bail!("Failed to wait hook result {}", e)
        }
    }
}

/// 프라이버시 모드 입력 훅을 제거하고 비활성화합니다.
///
/// 훅을 설치한 스레드에 WM_USER_EXIT_HOOK 메시지를 전송하여
/// 메시지 루프를 종료하고 훅을 제거하도록 신호를 보냅니다.
/// 훅이 설치되지 않은 상태면 무시합니다.
///
/// # 반환값
/// - Ok(()): 훅 제거 신호 전송 성공 또는 훅이 설치되지 않음
/// - Err: 메시지 전송 실패
pub fn unhook() -> ResultType<()> {
    unsafe {
        let cur_hook_thread_id = CUR_HOOK_THREAD_ID.lock().unwrap();
        // 훅이 설치되어 있으면 종료 신호 전송
        if *cur_hook_thread_id != 0 {
            if FALSE == PostThreadMessageA(*cur_hook_thread_id, WM_USER_EXIT_HOOK, 0, 0) {
                bail!(
                    "Failed to post message to exit hook, error {}",
                    Error::last_os_error()
                );
            }
        }
    }
    Ok(())
}

/// 저수준 키보드 훅 콜백 함수입니다.
///
/// 프라이버시 모드 중에 사용자의 키보드 입력을 모니터링합니다.
/// - Ctrl+P 조합으로 프라이버시 모드를 해제할 수 있습니다.
/// - Alt 키 (Alt+Tab 등)를 차단하여 다른 윈도우로 전환하는 것을 방지합니다.
/// - P 키와 Ctrl 키 외의 모든 키 입력을 차단합니다.
/// - 마우스나 enigo 라이브러리의 자동 입력은 통과시킵니다.
///
/// # 인자
/// - `code`: 훅 코드 (음수면 다음 훅으로 전달)
/// - `w_param`: 키 입력 메시지 (WM_KEYDOWN, WM_KEYUP 등)
/// - `l_param`: KBDLLHOOKSTRUCT 포인터 (키보드 정보)
///
/// # 반환값
/// - 0: 입력 처리를 계속 진행 (다음 훅으로 전달)
/// - 1: 입력을 차단 (처리 중지)
#[no_mangle]
pub extern "system" fn privacy_mode_hook_keyboard(
    code: c_int,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    // 훅 코드가 음수면 다음 훅으로 전달 (처리하지 않음)
    if code < 0 {
        unsafe {
            return CallNextHookEx(NULL as _, code, w_param, l_param);
        }
    }

    let ks = l_param as PKBDLLHOOKSTRUCT;
    let w_param2 = w_param as UINT;

    unsafe {
        // dwExtraInfo가 enigo 자동 입력 값이 아니면 (= 사용자 입력)
        if (*ks).dwExtraInfo != enigo::ENIGO_INPUT_EXTRA_VALUE {
            // Alt 키 차단: Alt+Tab 등으로 다른 윈도우 전환 방지
            if (*ks).flags & LLKHF_ALTDOWN == LLKHF_ALTDOWN {
                return 1;
            }

            match w_param2 {
                WM_KEYDOWN => {
                    // P(80), 좌측 Ctrl(162), 우측 Ctrl(163)만 허용
                    // 다른 모든 키는 차단
                    if ![80, 162, 163].contains(&(*ks).vkCode) {
                        return 1;
                    }

                    // Ctrl+P 입력 검사: 프라이버시 모드 해제 단축키
                    let cltr_down = (GetKeyState(VK_CONTROL) as u16) & (0x8000 as u16) > 0;
                    let key = LOBYTE((*ks).vkCode as _);
                    if cltr_down && (key == 'p' as u8 || key == 'P' as u8) {
                        // Ctrl+P 입력 감지: 프라이버시 모드 해제 실행
                        if let Some(Err(e)) = super::turn_off_privacy(
                            super::INVALID_PRIVACY_MODE_CONN_ID,
                            Some(super::PrivacyModeState::OffByPeer),
                        ) {
                            log::error!("Failed to off_privacy {}", e);
                        }
                    }
                }
                WM_KEYUP => {
                    log::trace!("WM_KEYUP {}", (*ks).vkCode);
                }
                _ => {
                    log::trace!("KEYBOARD OTHER {} {}", w_param2, (*ks).vkCode);
                }
            }
        }
    }
    unsafe { CallNextHookEx(NULL as _, code, w_param, l_param) }
}

/// 저수준 마우스 훅 콜백 함수입니다.
///
/// 프라이버시 모드 중에 사용자의 마우스 입력을 모니터링합니다.
/// - 사용자의 모든 마우스 입력(움직임, 클릭 등)을 차단합니다.
/// - enigo 라이브러리의 자동 마우스 입력은 통과시킵니다.
///
/// # 인자
/// - `code`: 훅 코드 (음수면 다음 훅으로 전달)
/// - `w_param`: 마우스 메시지 (WM_MOUSEMOVE, WM_LBUTTONDOWN 등)
/// - `l_param`: MOUSEHOOKSTRUCT 포인터 (마우스 정보)
///
/// # 반환값
/// - 0: 입력 처리를 계속 진행 (다음 훅으로 전달)
/// - 1: 입력을 차단 (처리 중지)
#[no_mangle]
pub extern "system" fn privacy_mode_hook_mouse(
    code: c_int,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    // 훅 코드가 음수면 다음 훅으로 전달 (처리하지 않음)
    if code < 0 {
        unsafe {
            return CallNextHookEx(NULL as _, code, w_param, l_param);
        }
    }

    let ms = l_param as PMOUSEHOOKSTRUCT;
    unsafe {
        // dwExtraInfo가 enigo 자동 입력 값이 아니면 (= 사용자 마우스 입력)
        // 모든 사용자 마우스 입력을 차단
        if (*ms).dwExtraInfo != enigo::ENIGO_INPUT_EXTRA_VALUE {
            return 1;
        }
    }
    unsafe { CallNextHookEx(NULL as _, code, w_param, l_param) }
}

// 테스트 모듈 (현재 비활성화)
mod test {
    #[test]
    fn privacy_hook() {
        // 프라이버시 모드 훅 테스트 코드
        // 실제 훅 설치/해제는 다음과 같이 동작합니다:
        // 1. privacy_hook::hook().unwrap() - 훅 설치
        // 2. 대기 시간 (메시지 루프 실행)
        // 3. privacy_hook::unhook().unwrap() - 훅 제거
    }
}
