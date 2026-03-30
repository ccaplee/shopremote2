use super::service::{EmptyExtraFieldService, GenericService, Service};
use hbb_common::{bail, dlopen::symbor::Library, log, ResultType};
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

/// 원격 프린터 서비스의 이름
pub const NAME: &'static str = "remote-printer";

/// 프린터 드라이버 어댑터 라이브러리 이름
const LIB_NAME_PRINTER_DRIVER_ADAPTER: &str = "printer_driver_adapter";

/// 프린터 드라이버 초기화 함수
/// 성공 시 0 반환, 그 외 오류 코드 반환
pub type Init = fn(tag_name: *const i8) -> i32;

/// 프린터 드라이버 종료 함수
pub type Uninit = fn();

/// 프린터 데이터 조회 함수
/// dur_mills: 마지막 dur_mills 밀리초 동안 생성된 파일 조회
/// data: 원시 프린터 데이터 (XPS 형식)
/// data_len: 원시 프린터 데이터의 길이
pub type GetPrnData = fn(dur_mills: u32, data: *mut *mut i8, data_len: *mut u32);

/// 프린터 데이터 해제 함수
/// GetPrnData()로 할당된 메모리 해제
pub type FreePrnData = fn(data: *mut i8);

macro_rules! make_lib_wrapper {
    ($($field:ident : $tp:ty),+) => {
        struct LibWrapper {
            _lib: Option<Library>,
            $($field: Option<$tp>),+
        }

        impl LibWrapper {
            fn new() -> Self {
                let lib_name = match get_lib_name() {
                    Ok(name) => name,
                    Err(e) => {
                        log::warn!("Failed to get lib name, {}", e);
                        return Self {
                            _lib: None,
                            $( $field: None ),+
                        };
                    }
                };
                let lib = match Library::open(&lib_name) {
                    Ok(lib) => Some(lib),
                    Err(e) => {
                        log::warn!("Failed to load library {}, {}", &lib_name, e);
                        None
                    }
                };

                $(let $field = if let Some(lib) = &lib {
                    match unsafe { lib.symbol::<$tp>(stringify!($field)) } {
                        Ok(m) => {
                            Some(*m)
                        },
                        Err(e) => {
                            log::warn!("Failed to load func {}, {}", stringify!($field), e);
                            None
                        }
                    }
                } else {
                    None
                };)+

                Self {
                    _lib: lib,
                    $( $field ),+
                }
            }
        }

        impl Default for LibWrapper {
            fn default() -> Self {
                Self::new()
            }
        }
    }
}

make_lib_wrapper!(
    init: Init,
    uninit: Uninit,
    get_prn_data: GetPrnData,
    free_prn_data: FreePrnData
);

lazy_static::lazy_static! {
    static ref LIB_WRAPPER: Arc<Mutex<LibWrapper>> = Default::default();
}

/// 프린터 드라이버 라이브러리 경로 조회
/// 현재 실행 파일의 디렉토리에서 printer_driver_adapter.dll 파일을 찾음
fn get_lib_name() -> ResultType<String> {
    let exe_file = std::env::current_exe()?;
    if let Some(cur_dir) = exe_file.parent() {
        let dll_name = format!("{}.dll", LIB_NAME_PRINTER_DRIVER_ADAPTER);
        let full_path = cur_dir.join(dll_name);
        if !full_path.exists() {
            bail!("{} 찾을 수 없음", full_path.to_string_lossy().as_ref());
        } else {
            Ok(full_path.to_string_lossy().into_owned())
        }
    } else {
        bail!(
            "잘못된 실행 파일 부모 디렉토리: {}",
            exe_file.to_string_lossy().as_ref()
        );
    }
}

/// 프린터 드라이버 초기화
/// app_name: 응용 프로그램 이름 (태그 이름으로 사용)
pub fn init(app_name: &str) -> ResultType<()> {
    let lib_wrapper = LIB_WRAPPER.lock().unwrap();
    let Some(fn_init) = lib_wrapper.init.as_ref() else {
        bail!("Init 함수 로드 실패");
    };

    let tag_name = std::ffi::CString::new(app_name)?;
    let ret = fn_init(tag_name.as_ptr());
    if ret != 0 {
        bail!("프린터 드라이버 초기화 실패");
    }
    Ok(())
}

/// 프린터 드라이버 종료
pub fn uninit() {
    let lib_wrapper = LIB_WRAPPER.lock().unwrap();
    if let Some(fn_uninit) = lib_wrapper.uninit.as_ref() {
        fn_uninit();
    }
}

/// 지정된 시간 범위 내의 프린터 데이터 조회
/// dur_mills: 조회 범위 (밀리초)
/// 반환값: XPS 형식의 프린터 데이터 바이트 배열
fn get_prn_data(dur_mills: u32) -> ResultType<Vec<u8>> {
    let lib_wrapper = LIB_WRAPPER.lock().unwrap();
    if let Some(fn_get_prn_data) = lib_wrapper.get_prn_data.as_ref() {
        let mut data = std::ptr::null_mut();
        let mut data_len = 0u32;
        fn_get_prn_data(dur_mills, &mut data, &mut data_len);
        if data.is_null() || data_len == 0 {
            return Ok(Vec::new());
        }
        let bytes =
            Vec::from(unsafe { std::slice::from_raw_parts(data as *const u8, data_len as usize) });
        lib_wrapper.free_prn_data.map(|f| f(data));
        Ok(bytes)
    } else {
        bail!("get_prn_file 함수 로드 실패");
    }
}

/// 프린터 서비스 생성
/// name: 서비스 이름
/// 반환값: 실행 중인 프린터 서비스
pub fn new(name: String) -> GenericService {
    let svc = EmptyExtraFieldService::new(name, false);
    GenericService::run(&svc.clone(), run);
    svc.sp
}

/// 프린터 데이터 수집 및 전송 루프
/// 주기적으로 프린터 데이터를 조회하고 서버로 전송
/// sp: 서비스 제어 객체 (종료 신호 감시)
fn run(sp: EmptyExtraFieldService) -> ResultType<()> {
    while sp.ok() {
        let bytes = get_prn_data(1000)?;
        if !bytes.is_empty() {
            log::info!("프린터 데이터 수신, 데이터 길이: {}", bytes.len());
            crate::server::on_printer_data(bytes);
        }
        thread::sleep(Duration::from_millis(300));
    }
    Ok(())
}
