use super::{PrivacyMode, INVALID_PRIVACY_MODE_CONN_ID};
use crate::{platform::windows::get_user_token, privacy_mode::PrivacyModeState};
use hbb_common::{allow_err, bail, log, ResultType};
use std::{
    ffi::CString,
    io::Error,
    time::{Duration, Instant},
};
use winapi::{
    shared::{
        minwindef::FALSE,
        ntdef::{HANDLE, NULL},
        windef::HWND,
    },
    um::{
        handleapi::CloseHandle,
        libloaderapi::{GetModuleHandleA, GetProcAddress},
        memoryapi::{VirtualAllocEx, WriteProcessMemory},
        processthreadsapi::{
            CreateProcessAsUserW, QueueUserAPC, ResumeThread, TerminateProcess,
            PROCESS_INFORMATION, STARTUPINFOW,
        },
        winbase::{WTSGetActiveConsoleSessionId, CREATE_SUSPENDED, DETACHED_PROCESS},
        winnt::{MEM_COMMIT, PAGE_READWRITE},
        winuser::*,
    },
};

// Magnifier API를 사용하는 프라이버시 모드 구현 식별자
pub(super) const PRIVACY_MODE_IMPL: &str = "privacy_mode_impl_mag";

// 원본 프로세스 실행 파일 (런타임 브로커)
pub const ORIGIN_PROCESS_EXE: &'static str = "C:\\Windows\\System32\\RuntimeBroker.exe";
// 프라이버시 모드 윈도우를 위해 주입할 프로세스 실행 파일 이름
pub const WIN_TOPMOST_INJECTED_PROCESS_EXE: &'static str = "RuntimeBroker_shopremote2.exe";
// 실제 주입될 프로세스 (위와 동일)
pub const INJECTED_PROCESS_EXE: &'static str = WIN_TOPMOST_INJECTED_PROCESS_EXE;
// 프라이버시 보호 윈도우의 클래스명/윈도우명
pub(super) const PRIVACY_WINDOW_NAME: &'static str = "ShopRemote2PrivacyWindow";

/// 프라이버시 모드 윈도우 프로세스의 핸들을 관리하는 구조체입니다.
///
/// 프라이버시 보호 윈도우를 실행하는 자식 프로세스의 스레드 및 프로세스 핸들을 보유합니다.
/// Drop 트레이트를 구현하여 소멸 시 자동으로 프로세스를 종료하고 핸들을 정리합니다.
struct WindowHandlers {
    /// 프로세스의 주 스레드 핸들 (0이면 유효하지 않음)
    hthread: u64,
    /// 프로세스 핸들 (0이면 유효하지 않음)
    hprocess: u64,
}

impl Drop for WindowHandlers {
    fn drop(&mut self) {
        self.reset();
    }
}

impl WindowHandlers {
    /// 프로세스를 종료하고 모든 핸들을 정리합니다.
    fn reset(&mut self) {
        unsafe {
            // 프로세스 핸들이 유효하면 프로세스 종료
            if self.hprocess != 0 {
                let _res = TerminateProcess(self.hprocess as _, 0);
                CloseHandle(self.hprocess as _);
            }
            self.hprocess = 0;
            // 스레드 핸들이 유효하면 정리
            if self.hthread != 0 {
                CloseHandle(self.hthread as _);
            }
            self.hthread = 0;
        }
    }

    /// 핸들이 모두 초기화 상태(0)인지 확인합니다.
    fn is_default(&self) -> bool {
        self.hthread == 0 && self.hprocess == 0
    }
}

/// Magnifier API와 윈도우 주입을 사용하는 프라이버시 모드 구현입니다.
///
/// 프라이버시 보호 윈도우 프로세스를 실행하고, DLL 주입을 통해
/// 화면을 캡처할 수 없도록 하는 윈도우를 최상단에 표시합니다.
/// 입력 훅을 통해 사용자 입력을 제어합니다.
pub struct PrivacyModeImpl {
    /// 프라이버시 모드 구현 식별자
    impl_key: String,
    /// 현재 활성화된 프라이버시 모드의 연결 ID
    conn_id: i32,
    /// 프라이버시 윈도우 프로세스 핸들 관리자
    handlers: WindowHandlers,
    /// 프라이버시 보호 윈도우의 핸들
    hwnd: u64,
}

impl PrivacyMode for PrivacyModeImpl {
    /// Magnifier 구현은 동기식 프라이버시 모드입니다.
    fn is_async_privacy_mode(&self) -> bool {
        false
    }

    /// 초기화 (필요한 작업 없음)
    fn init(&self) -> ResultType<()> {
        Ok(())
    }

    /// 프라이버시 모드 정리 및 종료
    fn clear(&mut self) {
        allow_err!(self.turn_off_privacy(self.conn_id, None));
    }

    /// 프라이버시 모드를 활성화합니다.
    ///
    /// # 동작 과정
    /// 1. 프라이버시 모드가 이미 활성화되었는지 확인
    /// 2. WindowInjection.dll 파일 존재 여부 확인
    /// 3. 프라이버시 윈도우 프로세스 시작
    /// 4. 키보드/마우스 입력 훅 설치
    /// 5. 프라이버시 보호 윈도우 표시
    ///
    /// # 인자
    /// - `conn_id`: 연결 ID
    ///
    /// # 반환값
    /// - Ok(true): 프라이버시 모드 활성화 성공
    /// - Ok(false): WindowInjection.dll이 없거나 찾을 수 없음
    /// - Err: 실패
    fn turn_on_privacy(&mut self, conn_id: i32) -> ResultType<bool> {
        if self.check_on_conn_id(conn_id)? {
            log::debug!("Privacy mode of conn {} is already on", conn_id);
            return Ok(true);
        }

        // WindowInjection.dll 존재 확인
        let exe_file = std::env::current_exe()?;
        if let Some(cur_dir) = exe_file.parent() {
            if !cur_dir.join("WindowInjection.dll").exists() {
                return Ok(false);
            }
        } else {
            bail!(
                "Invalid exe parent for {}",
                exe_file.to_string_lossy().as_ref()
            );
        }

        // 프라이버시 윈도우 프로세스가 아직 실행되지 않으면 시작
        if self.handlers.is_default() {
            log::info!("turn_on_privacy, dll not found when started, try start");
            self.start()?;
            std::thread::sleep(std::time::Duration::from_millis(1_000));
        }

        // 프라이버시 보호 윈도우 찾기
        let hwnd = wait_find_privacy_hwnd(0)?;
        if hwnd.is_null() {
            bail!("No privacy window created");
        }
        // 입력 훅 설치
        super::win_input::hook()?;
        unsafe {
            ShowWindow(hwnd as _, SW_SHOW);
        }
        self.conn_id = conn_id;
        self.hwnd = hwnd as _;
        Ok(true)
    }

    /// 프라이버시 모드를 비활성화합니다.
    ///
    /// # 동작 과정
    /// 1. 연결 ID 검증
    /// 2. 입력 훅 제거
    /// 3. 프라이버시 보호 윈도우 숨김
    /// 4. 상태 변화 알림 (필요시)
    ///
    /// # 인자
    /// - `conn_id`: 연결 ID
    /// - `state`: 프라이버시 모드 종료 상태 (선택사항)
    fn turn_off_privacy(
        &mut self,
        conn_id: i32,
        state: Option<PrivacyModeState>,
    ) -> ResultType<()> {
        self.check_off_conn_id(conn_id)?;
        // 입력 훅 제거
        super::win_input::unhook()?;

        // 프라이버시 보호 윈도우 숨김
        unsafe {
            let hwnd = wait_find_privacy_hwnd(0)?;
            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_HIDE);
            }
        }

        // 상태 변화 알림 (필요시)
        if self.conn_id != INVALID_PRIVACY_MODE_CONN_ID {
            if let Some(state) = state {
                allow_err!(super::set_privacy_mode_state(
                    conn_id,
                    state,
                    PRIVACY_MODE_IMPL.to_string(),
                    1_000
                ));
            }
            self.conn_id = INVALID_PRIVACY_MODE_CONN_ID.to_owned();
        }

        Ok(())
    }

    /// 현재 활성화된 연결의 ID를 반환합니다.
    #[inline]
    fn pre_conn_id(&self) -> i32 {
        self.conn_id
    }

    /// 프라이버시 모드 구현 식별자를 반환합니다.
    #[inline]
    fn get_impl_key(&self) -> &str {
        &self.impl_key
    }
}

impl PrivacyModeImpl {
    /// 새로운 프라이버시 모드 구현 인스턴스를 생성합니다.
    pub fn new(impl_key: &str) -> Self {
        Self {
            impl_key: impl_key.to_owned(),
            conn_id: INVALID_PRIVACY_MODE_CONN_ID,
            handlers: WindowHandlers {
                hthread: 0,
                hprocess: 0,
            },
            hwnd: 0,
        }
    }

    /// 현재 프라이버시 보호 윈도우의 핸들을 반환합니다.
    #[inline]
    pub fn get_hwnd(&self) -> u64 {
        self.hwnd
    }

    /// 프라이버시 모드 윈도우 프로세스를 시작합니다.
    ///
    /// # 동작 과정
    /// 1. 이미 프로세스가 실행 중인지 확인
    /// 2. 브로커 프로세스 업데이트 확인
    /// 3. WindowInjection.dll 존재 확인
    /// 4. 프라이버시 윈도우가 이미 생성되었는지 확인
    /// 5. 주입할 프로세스 실행 파일의 명령줄 생성
    /// 6. 현재 사용자의 토큰 획득
    /// 7. 해당 사용자 토큰으로 프로세스 생성 (서스펜드 상태)
    /// 8. DLL 주입 수행
    /// 9. 프로세스 재개
    /// 10. 프라이버시 보호 윈도우 생성 확인
    ///
    /// # 반환값
    /// - Ok(()): 프로세스 시작 성공 또는 이미 실행 중
    /// - Err: 각 단계의 실패 (파일 없음, 프로세스 생성 실패 등)
    pub fn start(&mut self) -> ResultType<()> {
        // 프로세스가 이미 실행 중이면 반환
        if self.handlers.hprocess != 0 {
            return Ok(());
        }

        log::info!("Start privacy mode window broker, check_update_broker_process");
        // 브로커 프로세스 업데이트 확인 (실패해도 계속 진행)
        if let Err(e) = crate::platform::windows::check_update_broker_process() {
            log::warn!(
                "Failed to check update broker process. Privacy mode may not work properly. {}",
                e
            );
        }

        // 현재 실행 파일의 디렉토리 획득
        let exe_file = std::env::current_exe()?;
        let Some(cur_dir) = exe_file.parent() else {
            bail!("Cannot get parent of current exe file");
        };

        // WindowInjection.dll 존재 확인
        let dll_file = cur_dir.join("WindowInjection.dll");
        if !dll_file.exists() {
            bail!(
                "Failed to find required file {}",
                dll_file.to_string_lossy().as_ref()
            );
        }

        // 프라이버시 보호 윈도우가 이미 생성되었는지 확인
        let hwnd = wait_find_privacy_hwnd(1_000)?;
        if !hwnd.is_null() {
            log::info!("Privacy window is ready");
            return Ok(());
        }

        // 프로세스 실행 파일 명령줄 생성
        let cmdline = cur_dir
            .join(INJECTED_PROCESS_EXE)
            .to_string_lossy()
            .to_string();

        unsafe {
            // 명령줄을 UTF-16으로 인코딩
            let cmd_utf16: Vec<u16> = cmdline.encode_utf16().chain(Some(0).into_iter()).collect();

            // 프로세스 시작 정보 초기화
            let mut start_info = STARTUPINFOW {
                cb: 0,
                lpReserved: NULL as _,
                lpDesktop: NULL as _,
                lpTitle: NULL as _,
                dwX: 0,
                dwY: 0,
                dwXSize: 0,
                dwYSize: 0,
                dwXCountChars: 0,
                dwYCountChars: 0,
                dwFillAttribute: 0,
                dwFlags: 0,
                wShowWindow: 0,
                cbReserved2: 0,
                lpReserved2: NULL as _,
                hStdInput: NULL as _,
                hStdOutput: NULL as _,
                hStdError: NULL as _,
            };
            // 프로세스 정보 초기화
            let mut proc_info = PROCESS_INFORMATION {
                hProcess: NULL as _,
                hThread: NULL as _,
                dwProcessId: 0,
                dwThreadId: 0,
            };

            // 현재 사용자의 세션 ID 및 토큰 획득
            let session_id = WTSGetActiveConsoleSessionId();
            let token = get_user_token(session_id, true);
            if token.is_null() {
                bail!("Failed to get token of current user");
            }

            // 사용자 토큰으로 프로세스 생성 (서스펜드 상태)
            let create_res = CreateProcessAsUserW(
                token,
                NULL as _,
                cmd_utf16.as_ptr() as _,
                NULL as _,
                NULL as _,
                FALSE,
                CREATE_SUSPENDED | DETACHED_PROCESS,
                NULL,
                NULL as _,
                &mut start_info,
                &mut proc_info,
            );
            CloseHandle(token);
            if 0 == create_res {
                bail!(
                    "Failed to create privacy window process {}, error {}",
                    cmdline,
                    Error::last_os_error()
                );
            };

            // DLL 주입 수행
            inject_dll(
                proc_info.hProcess,
                proc_info.hThread,
                dll_file.to_string_lossy().as_ref(),
            )?;

            // 프로세스 재개
            if 0xffffffff == ResumeThread(proc_info.hThread) {
                CloseHandle(proc_info.hThread);
                CloseHandle(proc_info.hProcess);

                bail!(
                    "Failed to create privacy window process, error {}",
                    Error::last_os_error()
                );
            }

            // 프로세스 및 스레드 핸들 저장
            self.handlers.hthread = proc_info.hThread as _;
            self.handlers.hprocess = proc_info.hProcess as _;

            // 프라이버시 보호 윈도우 생성 확인
            let hwnd = wait_find_privacy_hwnd(1_000)?;
            if hwnd.is_null() {
                bail!("Failed to get hwnd after started");
            }
        }

        Ok(())
    }

    /// 프라이버시 모드 윈도우 프로세스를 정지합니다.
    #[inline]
    pub fn stop(&mut self) {
        self.handlers.reset();
    }
}

impl Drop for PrivacyModeImpl {
    /// 인스턴스 소멸 시 프라이버시 모드를 비활성화합니다.
    fn drop(&mut self) {
        if self.conn_id != INVALID_PRIVACY_MODE_CONN_ID {
            allow_err!(self.turn_off_privacy(self.conn_id, None));
        }
    }
}

/// 대상 프로세스에 DLL을 주입합니다.
///
/// # 동작 과정
/// 1. DLL 파일 경로를 UTF-16으로 인코딩
/// 2. 대상 프로세스 메모리에 읽기/쓰기 권한으로 버퍼 할당
/// 3. DLL 파일 경로를 할당된 메모리에 기록
/// 4. kernel32.dll에서 LoadLibraryW 함수 주소 획득
/// 5. APC(Asynchronous Procedure Call)를 통해 대상 스레드에서 LoadLibraryW 실행
///    (할당된 메모리의 DLL 경로를 매개변수로 전달)
///
/// # 인자
/// - `hproc`: 대상 프로세스 핸들
/// - `hthread`: 대상 스레드 핸들 (APC 실행 대상)
/// - `dll_file`: 주입할 DLL 파일의 경로
///
/// # 반환값
/// - Ok(()): DLL 주입 성공
/// - Err: 각 단계의 실패 (메모리 할당, 함수 획득 등)
unsafe fn inject_dll<'a>(hproc: HANDLE, hthread: HANDLE, dll_file: &'a str) -> ResultType<()> {
    // DLL 파일 경로를 UTF-16으로 인코딩
    let dll_file_utf16: Vec<u16> = dll_file.encode_utf16().chain(Some(0).into_iter()).collect();

    // 대상 프로세스 메모리에 DLL 경로 저장 공간 할당
    let buf = VirtualAllocEx(
        hproc,
        NULL as _,
        dll_file_utf16.len() * 2,
        MEM_COMMIT,
        PAGE_READWRITE,
    );
    if buf.is_null() {
        bail!("Failed VirtualAllocEx");
    }

    // DLL 경로를 할당된 메모리에 기록
    let mut written: usize = 0;
    if 0 == WriteProcessMemory(
        hproc,
        buf,
        dll_file_utf16.as_ptr() as _,
        dll_file_utf16.len() * 2,
        &mut written,
    ) {
        bail!("Failed WriteProcessMemory");
    }

    // kernel32.dll 모듈 핸들 획득
    let kernel32_modulename = CString::new("kernel32")?;
    let hmodule = GetModuleHandleA(kernel32_modulename.as_ptr() as _);
    if hmodule.is_null() {
        bail!("Failed GetModuleHandleA");
    }

    // LoadLibraryW 함수 주소 획득
    let load_librarya_name = CString::new("LoadLibraryW")?;
    let load_librarya = GetProcAddress(hmodule, load_librarya_name.as_ptr() as _);
    if load_librarya.is_null() {
        bail!("Failed GetProcAddress of LoadLibraryW");
    }

    // APC를 통해 대상 스레드에서 LoadLibraryW를 실행
    // 할당된 메모리의 DLL 경로를 매개변수로 전달
    if 0 == QueueUserAPC(Some(std::mem::transmute(load_librarya)), hthread, buf as _) {
        bail!("Failed QueueUserAPC");
    }

    Ok(())
}

/// 프라이버시 보호 윈도우를 찾습니다.
///
/// # 동작 과정
/// 1. 주어진 시간 동안 반복적으로 프라이버시 윈도우를 검색
/// 2. 100ms 간격으로 재시도
/// 3. 제한 시간 초과 시 NULL 반환
///
/// # 인자
/// - `msecs`: 검색 시간 제한 (밀리초). 0이면 한 번만 시도
///
/// # 반환값
/// - Ok(hwnd): 프라이버시 윈도우 핸들 (찾지 못하면 NULL)
/// - Err: 프로세스 오류
pub(super) fn wait_find_privacy_hwnd(msecs: u128) -> ResultType<HWND> {
    let tm_begin = Instant::now();
    let wndname = CString::new(PRIVACY_WINDOW_NAME)?;
    loop {
        unsafe {
            // 프라이버시 윈도우명으로 윈도우 검색
            let hwnd = FindWindowA(NULL as _, wndname.as_ptr() as _);
            if !hwnd.is_null() {
                return Ok(hwnd);
            }
        }

        // 시간 초과 확인
        if msecs == 0 || tm_begin.elapsed().as_millis() > msecs {
            return Ok(NULL as _);
        }

        // 재시도 전 대기
        std::thread::sleep(Duration::from_millis(100));
    }
}
