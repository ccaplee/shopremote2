use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Instant,
};
use winapi::{
    shared::minwindef::{DWORD, FALSE, TRUE},
    um::{
        handleapi::CloseHandle,
        pdh::{
            PdhAddEnglishCounterA, PdhCloseQuery, PdhCollectQueryData, PdhCollectQueryDataEx,
            PdhGetFormattedCounterValue, PdhOpenQueryA, PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE,
            PDH_HCOUNTER, PDH_HQUERY,
        },
        synchapi::{CreateEventA, WaitForSingleObject},
        sysinfoapi::VerSetConditionMask,
        winbase::{VerifyVersionInfoW, INFINITE, WAIT_OBJECT_0},
        winnt::{
            HANDLE, OSVERSIONINFOEXW, VER_BUILDNUMBER, VER_GREATER_EQUAL, VER_MAJORVERSION,
            VER_MINORVERSION, VER_SERVICEPACKMAJOR, VER_SERVICEPACKMINOR,
        },
    },
};

lazy_static::lazy_static! {
    /// 최근 1분간의 CPU 사용률 캐시 (값, 시간)
    static ref CPU_USAGE_ONE_MINUTE: Arc<Mutex<Option<(f64, Instant)>>> = Arc::new(Mutex::new(None));
}

/// RAII 패턴을 사용하는 Windows 핸들 래퍼입니다.
/// 스코프를 벗어나면 자동으로 핸들을 닫습니다.
/// 참고: https://github.com/mgostIH/process_list/blob/master/src/windows/mod.rs
#[repr(transparent)]
pub struct RAIIHandle(pub HANDLE);

impl Drop for RAIIHandle {
    fn drop(&mut self) {
        // 디버거 실행 시 제외하고는 문제가 없습니다.
        unsafe { CloseHandle(self.0) };
    }
}

/// RAII 패턴을 사용하는 PDH 쿼리 핸들 래퍼입니다.
#[repr(transparent)]
pub(self) struct RAIIPDHQuery(pub PDH_HQUERY);

impl Drop for RAIIPDHQuery {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.0) };
    }
}

/// CPU 성능 모니터링을 시작합니다.
/// Windows 성능 데이터(Performance Data)를 주기적으로 수집합니다.
/// 참고:
/// - https://learn.microsoft.com/en-us/windows/win32/perfctrs/collecting-performance-data
/// - https://learn.microsoft.com/en-us/windows/win32/api/pdh/nf-pdh-pdhcollectquerydataex
///
/// 왜 작업 관리자보다 낮은 값이 나오나:
/// - https://aaron-margosis.medium.com/task-managers-cpu-numbers-are-all-but-meaningless-2d165b421e43
/// - 따라서 작업 관리자보다 프로세스 탐색기와 비교하는 것이 정확합니다.
pub fn start_cpu_performance_monitor() {

    let f = || unsafe {
        // 로드 평균 또는 CPU 사용률을 수집합니다 (prime95로 테스트됨).
        // CPU 사용률을 선호합니다 (프로세스 탐색기와 비교 가능).
        // const COUNTER_PATH: &'static str = "\\System\\Processor Queue Length\0";
        const COUNTER_PATH: &'static str = "\\Processor(_total)\\% Processor Time\0";
        const SAMPLE_INTERVAL: DWORD = 2;  // 2초 간격으로 샘플링

        let mut ret;
        let mut query: PDH_HQUERY = std::mem::zeroed();
        ret = PdhOpenQueryA(std::ptr::null() as _, 0, &mut query);
        if ret != 0 {
            log::error!("PdhOpenQueryA failed: 0x{:X}", ret);
            return;
        }
        let _query = RAIIPDHQuery(query);
        let mut counter: PDH_HCOUNTER = std::mem::zeroed();
        ret = PdhAddEnglishCounterA(query, COUNTER_PATH.as_ptr() as _, 0, &mut counter);
        if ret != 0 {
            log::error!("PdhAddEnglishCounterA failed: 0x{:X}", ret);
            return;
        }
        ret = PdhCollectQueryData(query);
        if ret != 0 {
            log::error!("PdhCollectQueryData failed: 0x{:X}", ret);
            return;
        }
        let mut _counter_type: DWORD = 0;
        let mut counter_value: PDH_FMT_COUNTERVALUE = std::mem::zeroed();
        let event = CreateEventA(std::ptr::null_mut(), FALSE, FALSE, std::ptr::null() as _);
        if event.is_null() {
            log::error!("CreateEventA failed");
            return;
        }
        let _event: RAIIHandle = RAIIHandle(event);
        ret = PdhCollectQueryDataEx(query, SAMPLE_INTERVAL, event);
        if ret != 0 {
            log::error!("PdhCollectQueryDataEx failed: 0x{:X}", ret);
            return;
        }

        let mut queue: VecDeque<f64> = VecDeque::new();
        let mut recent_valid: VecDeque<bool> = VecDeque::new();
        loop {
            // latest one minute
            if queue.len() == 31 {
                queue.pop_front();
            }
            if recent_valid.len() == 31 {
                recent_valid.pop_front();
            }
            // allow get value within one minute
            if queue.len() > 0 && recent_valid.iter().filter(|v| **v).count() > queue.len() / 2 {
                let sum: f64 = queue.iter().map(|f| f.to_owned()).sum();
                let avg = sum / (queue.len() as f64);
                *CPU_USAGE_ONE_MINUTE.lock().unwrap() = Some((avg, Instant::now()));
            } else {
                *CPU_USAGE_ONE_MINUTE.lock().unwrap() = None;
            }
            if WAIT_OBJECT_0 != WaitForSingleObject(event, INFINITE) {
                recent_valid.push_back(false);
                continue;
            }
            if PdhGetFormattedCounterValue(
                counter,
                PDH_FMT_DOUBLE,
                &mut _counter_type,
                &mut counter_value,
            ) != 0
                || counter_value.CStatus != 0
            {
                recent_valid.push_back(false);
                continue;
            }
            queue.push_back(counter_value.u.doubleValue().clone());
            recent_valid.push_back(true);
        }
    };
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::thread::spawn(f);
    });
}

/// 최근 1분간의 CPU 사용률을 반환합니다.
/// 30초 이내에 업데이트된 데이터만 반환합니다 (오래된 데이터는 None).
pub fn cpu_uage_one_minute() -> Option<f64> {
    let v = CPU_USAGE_ONE_MINUTE.lock().unwrap().clone();
    if let Some((v, instant)) = v {
        if instant.elapsed().as_secs() < 30 {
            return Some(v);
        }
    }
    None
}

/// CPU 사용률을 동기화합니다.
/// 서버에서 보낸 CPU 사용률 값으로 캐시를 업데이트합니다.
pub fn sync_cpu_usage(cpu_usage: Option<f64>) {
    let v = match cpu_usage {
        Some(cpu_usage) => Some((cpu_usage, Instant::now())),
        None => None,
    };
    *CPU_USAGE_ONE_MINUTE.lock().unwrap() = v;
    log::info!("cpu usage synced: {:?}", cpu_usage);
}

/// Windows 버전이 지정된 버전 이상인지 확인합니다.
/// 참고:
/// - https://learn.microsoft.com/en-us/windows/win32/sysinfo/targeting-your-application-at-windows-8-1
/// - https://github.com/nodejs/node-convergence-archive/blob/e11fe0c2777561827cdb7207d46b0917ef3c42a7/deps/uv/src/win/util.c#L780
pub fn is_windows_version_or_greater(
    os_major: u32,
    os_minor: u32,
    build_number: u32,
    service_pack_major: u32,
    service_pack_minor: u32,
) -> bool {
    let mut osvi: OSVERSIONINFOEXW = unsafe { std::mem::zeroed() };
    osvi.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOEXW>() as DWORD;
    osvi.dwMajorVersion = os_major as _;
    osvi.dwMinorVersion = os_minor as _;
    osvi.dwBuildNumber = build_number as _;
    osvi.wServicePackMajor = service_pack_major as _;
    osvi.wServicePackMinor = service_pack_minor as _;

    let result = unsafe {
        let mut condition_mask = 0;
        let op = VER_GREATER_EQUAL;
        condition_mask = VerSetConditionMask(condition_mask, VER_MAJORVERSION, op);
        condition_mask = VerSetConditionMask(condition_mask, VER_MINORVERSION, op);
        condition_mask = VerSetConditionMask(condition_mask, VER_BUILDNUMBER, op);
        condition_mask = VerSetConditionMask(condition_mask, VER_SERVICEPACKMAJOR, op);
        condition_mask = VerSetConditionMask(condition_mask, VER_SERVICEPACKMINOR, op);

        VerifyVersionInfoW(
            &mut osvi as *mut OSVERSIONINFOEXW,
            VER_MAJORVERSION
                | VER_MINORVERSION
                | VER_BUILDNUMBER
                | VER_SERVICEPACKMAJOR
                | VER_SERVICEPACKMINOR,
            condition_mask,
        )
    };

    result == TRUE
}
