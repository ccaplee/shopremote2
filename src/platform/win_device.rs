// Windows 디바이스 드라이버 설치 및 관리를 위한 모듈
// Windows API(SetupDI*, CreateFile 등)를 사용하여 플러그 앤 플레이 디바이스의 설치, 제거, I/O 제어를 수행합니다.

use hbb_common::{log, thiserror};
use std::{
    ffi::OsStr,
    io,
    ops::{Deref, DerefMut},
    os::windows::ffi::OsStrExt,
    ptr::null_mut,
    result::Result,
};
use winapi::{
    shared::{
        guiddef::GUID,
        minwindef::{BOOL, DWORD, FALSE, MAX_PATH, PBOOL, TRUE},
        ntdef::{HANDLE, LPCWSTR, NULL},
        windef::HWND,
        winerror::{ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS},
    },
    um::{
        cfgmgr32::MAX_DEVICE_ID_LEN,
        fileapi::{CreateFileW, OPEN_EXISTING},
        handleapi::{CloseHandle, INVALID_HANDLE_VALUE},
        ioapiset::DeviceIoControl,
        setupapi::*,
        winnt::{GENERIC_READ, GENERIC_WRITE},
    },
};

// Newdev.dll의 UpdateDriverForPlugAndPlayDevicesW 함수를 외부에서 호출하기 위한 선언
// 드라이버 설치를 수행하는 핵심 Windows API입니다.
#[link(name = "Newdev")]
extern "system" {
    fn UpdateDriverForPlugAndPlayDevicesW(
        hwnd_parent: HWND,
        hardware_id: LPCWSTR,
        full_inf_path: LPCWSTR,
        install_flags: DWORD,
        b_reboot_required: PBOOL,
    ) -> BOOL;
}

// Windows API 호출 중 발생하는 오류를 나타내는 열거형
// WinApiLastErr: os::Error를 포함하는 일반적인 Windows API 오류
// WinApiErrCode: 특정 오류 코드를 반환하는 Windows API 오류
// Raw: 그 외의 사용자 정의 오류 메시지
#[derive(thiserror::Error, Debug)]
pub enum DeviceError {
    #[error("Failed to call {0}, {1:?}")]
    WinApiLastErr(String, io::Error),
    #[error("Failed to call {0}, returns {1}")]
    WinApiErrCode(String, DWORD),
    #[error("{0}")]
    Raw(String),
}

impl DeviceError {
    // 주어진 API 이름으로 마지막 OS 오류를 포함하는 DeviceError를 생성합니다.
    #[inline]
    fn new_api_last_err(api: &str) -> Self {
        Self::WinApiLastErr(api.to_string(), io::Error::last_os_error())
    }
}

// Windows API로부터 반환받은 HDEVINFO 핸들을 래핑하고 자동 리소스 관리를 제공하는 구조체
// Drop 트레이트 구현으로 인해 스코프 벗어날 때 자동으로 SetupDiDestroyDeviceInfoList를 호출합니다.
struct DeviceInfo(HDEVINFO);

impl DeviceInfo {
    // 지정된 클래스 GUID를 기반으로 새로운 디바이스 정보 리스트를 생성합니다.
    // SetupDiCreateDeviceInfoList를 호출하여 HDEVINFO 핸들을 획득합니다.
    fn setup_di_create_device_info_list(class_guid: &mut GUID) -> Result<Self, DeviceError> {
        let dev_info = unsafe { SetupDiCreateDeviceInfoList(class_guid, null_mut()) };
        if dev_info == null_mut() {
            return Err(DeviceError::new_api_last_err("SetupDiCreateDeviceInfoList"));
        }

        Ok(Self(dev_info))
    }

    // 지정된 클래스 GUID와 플래그를 기반으로 디바이스 정보 리스트를 조회합니다.
    // DIGCF_PRESENT | DIGCF_DEVICEINTERFACE 같은 플래그와 함께 특정 디바이스를 필터링할 수 있습니다.
    fn setup_di_get_class_devs_ex_w(
        class_guid: *const GUID,
        flags: DWORD,
    ) -> Result<Self, DeviceError> {
        let dev_info = unsafe {
            SetupDiGetClassDevsExW(
                class_guid,
                null_mut(),
                null_mut(),
                flags,
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if dev_info == null_mut() {
            return Err(DeviceError::new_api_last_err("SetupDiGetClassDevsExW"));
        }
        Ok(Self(dev_info))
    }
}

// DeviceInfo가 스코프를 벗어날 때 Windows API의 SetupDiDestroyDeviceInfoList를 호출하여
// 디바이스 정보 리스트 핸들을 정리합니다. (RAII 패턴)
impl Drop for DeviceInfo {
    fn drop(&mut self) {
        unsafe {
            SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}

// DeviceInfo에서 내부의 HDEVINFO 핸들로 자동 변환을 가능하게 합니다.
// Windows API 함수 호출 시 * 연산자로 접근할 수 있도록 합니다.
impl Deref for DeviceInfo {
    type Target = HDEVINFO;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// DeviceInfo에서 내부의 HDEVINFO 핸들을 가변으로 접근할 수 있게 합니다.
impl DerefMut for DeviceInfo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// INF 파일을 통해 드라이버를 설치합니다.
// 단계:
// 1. INF 파일에서 클래스 GUID를 추출합니다.
// 2. 새로운 디바이스 정보 리스트를 생성합니다.
// 3. 디바이스를 등록하고 하드웨어 ID를 설정합니다.
// 4. UpdateDriverForPlugAndPlayDevicesW를 호출하여 실제 드라이버 설치를 수행합니다.
// 5. 재부팅이 필요한지 여부를 반환합니다.
pub unsafe fn install_driver(
    inf_path: &str,
    hardware_id: &str,
    reboot_required: &mut bool,
) -> Result<(), DeviceError> {
    // INF 파일 경로를 UTF-16 인코딩된 와이드 문자열로 변환합니다.
    let driver_inf_path = OsStr::new(inf_path)
        .encode_wide()
        .chain(Some(0).into_iter())
        .collect::<Vec<u16>>();
    // 하드웨어 ID를 UTF-16 인코딩된 와이드 문자열로 변환합니다.
    let hardware_id = OsStr::new(hardware_id)
        .encode_wide()
        .chain(Some(0).into_iter())
        .collect::<Vec<u16>>();

    let mut class_guid: GUID = std::mem::zeroed();
    let mut class_name: [u16; 32] = [0; 32];

    // INF 파일에서 디바이스 클래스 GUID와 클래스 이름을 추출합니다.
    if SetupDiGetINFClassW(
        driver_inf_path.as_ptr(),
        &mut class_guid,
        class_name.as_mut_ptr(),
        class_name.len() as _,
        null_mut(),
    ) == FALSE
    {
        return Err(DeviceError::new_api_last_err("SetupDiGetINFClassW"));
    }

    // 추출된 클래스 GUID를 기반으로 디바이스 정보 리스트를 생성합니다.
    let dev_info = DeviceInfo::setup_di_create_device_info_list(&mut class_guid)?;

    // 새로운 디바이스 정보 구조체를 초기화합니다.
    let mut dev_info_data = SP_DEVINFO_DATA {
        cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as _,
        ClassGuid: class_guid,
        DevInst: 0,
        Reserved: 0,
    };
    // 디바이스 정보 리스트에 새로운 디바이스를 생성합니다.
    // DICD_GENERATE_ID 플래그를 사용하여 자동으로 디바이스 ID를 생성합니다.
    if SetupDiCreateDeviceInfoW(
        *dev_info,
        class_name.as_ptr(),
        &class_guid,
        null_mut(),
        null_mut(),
        DICD_GENERATE_ID,
        &mut dev_info_data,
    ) == FALSE
    {
        return Err(DeviceError::new_api_last_err("SetupDiCreateDeviceInfoW"));
    }

    // 디바이스 레지스트리에 하드웨어 ID를 설정합니다.
    // 이를 통해 Windows가 올바른 드라이버를 식별할 수 있습니다.
    if SetupDiSetDeviceRegistryPropertyW(
        *dev_info,
        &mut dev_info_data,
        SPDRP_HARDWAREID,
        hardware_id.as_ptr() as _,
        (hardware_id.len() * 2) as _,
    ) == FALSE
    {
        return Err(DeviceError::new_api_last_err(
            "SetupDiSetDeviceRegistryPropertyW",
        ));
    }

    // DIF_REGISTERDEVICE 설치 함수를 호출하여 디바이스를 등록합니다.
    if SetupDiCallClassInstaller(DIF_REGISTERDEVICE, *dev_info, &mut dev_info_data) == FALSE {
        return Err(DeviceError::new_api_last_err("SetupDiCallClassInstaller"));
    }

    // UpdateDriverForPlugAndPlayDevicesW를 호출하여 실제 드라이버 설치를 수행합니다.
    let mut reboot_required_ = FALSE;
    if UpdateDriverForPlugAndPlayDevicesW(
        null_mut(),
        hardware_id.as_ptr(),
        driver_inf_path.as_ptr(),
        1,
        &mut reboot_required_,
    ) == FALSE
    {
        return Err(DeviceError::new_api_last_err(
            "UpdateDriverForPlugAndPlayDevicesW",
        ));
    }
    *reboot_required = reboot_required_ == TRUE;

    Ok(())
}

// 주어진 디바이스의 하드웨어 ID가 지정된 값과 일치하는지 확인합니다.
// 디바이스 레지스트리에서 현재 하드웨어 ID를 읽고 비교합니다.
unsafe fn is_same_hardware_id(
    dev_info: &DeviceInfo,
    devinfo_data: &mut SP_DEVINFO_DATA,
    hardware_id: &str,
) -> Result<bool, DeviceError> {
    // 디바이스의 하드웨어 ID를 저장할 버퍼를 할당합니다.
    let mut cur_hardware_id = [0u16; MAX_DEVICE_ID_LEN];
    // 디바이스 레지스트리에서 하드웨어 ID 속성을 읽습니다.
    if SetupDiGetDeviceRegistryPropertyW(
        **dev_info,
        devinfo_data,
        SPDRP_HARDWAREID,
        null_mut(),
        cur_hardware_id.as_mut_ptr() as _,
        cur_hardware_id.len() as _,
        null_mut(),
    ) == FALSE
    {
        return Err(DeviceError::new_api_last_err(
            "SetupDiGetDeviceRegistryPropertyW",
        ));
    }

    // UTF-16 와이드 문자 배열을 문자열로 변환하고 null 종료자를 제거합니다.
    let cur_hardware_id = String::from_utf16_lossy(&cur_hardware_id)
        .trim_end_matches(char::from(0))
        .to_string();
    // 읽은 하드웨어 ID와 입력된 값을 비교합니다.
    Ok(cur_hardware_id == hardware_id)
}

// 주어진 하드웨어 ID를 가진 드라이버를 시스템에서 제거합니다.
// 단계:
// 1. 시스템의 모든 디바이스를 조회합니다.
// 2. 일치하는 하드웨어 ID를 가진 디바이스를 찾습니다.
// 3. 각 디바이스에 대해 DIF_REMOVE 설치 함수를 호출하여 제거합니다.
// 4. 재부팅이 필요한지 확인하고 반환합니다.
pub unsafe fn uninstall_driver(
    hardware_id: &str,
    reboot_required: &mut bool,
) -> Result<(), DeviceError> {
    // 시스템의 모든 클래스의 모든 디바이스 정보를 조회합니다.
    let dev_info =
        DeviceInfo::setup_di_get_class_devs_ex_w(null_mut(), DIGCF_ALLCLASSES | DIGCF_PRESENT)?;

    // 디바이스 정보 리스트의 상세 정보를 조회합니다.
    let mut device_info_list_detail = SP_DEVINFO_LIST_DETAIL_DATA_W {
        cbSize: std::mem::size_of::<SP_DEVINFO_LIST_DETAIL_DATA_W>() as _,
        ClassGuid: std::mem::zeroed(),
        RemoteMachineHandle: null_mut(),
        RemoteMachineName: [0; SP_MAX_MACHINENAME_LENGTH],
    };
    if SetupDiGetDeviceInfoListDetailW(*dev_info, &mut device_info_list_detail) == FALSE {
        return Err(DeviceError::new_api_last_err(
            "SetupDiGetDeviceInfoListDetailW",
        ));
    }

    // 각 디바이스를 처리할 정보 구조체를 초기화합니다.
    let mut devinfo_data = SP_DEVINFO_DATA {
        cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as _,
        ClassGuid: std::mem::zeroed(),
        DevInst: 0,
        Reserved: 0,
    };

    // 시스템의 모든 디바이스를 순회합니다.
    let mut device_index = 0;
    loop {
        // 다음 디바이스 정보를 열거합니다.
        if SetupDiEnumDeviceInfo(*dev_info, device_index, &mut devinfo_data) == FALSE {
            let err = io::Error::last_os_error();
            // ERROR_NO_MORE_ITEMS는 모든 디바이스를 순회했다는 의미이므로 루프를 종료합니다.
            if err.raw_os_error() == Some(ERROR_NO_MORE_ITEMS as _) {
                break;
            }
            return Err(DeviceError::WinApiLastErr(
                "SetupDiEnumDeviceInfo".to_string(),
                err,
            ));
        }

        // 현재 디바이스의 하드웨어 ID가 찾는 ID와 일치하는지 확인합니다.
        match is_same_hardware_id(&dev_info, &mut devinfo_data, hardware_id) {
            Ok(false) => {
                // 일치하지 않으면 다음 디바이스로 진행합니다.
                device_index += 1;
                continue;
            }
            Err(e) => {
                // 오류 발생 시 로그하고 다음 디바이스로 진행합니다.
                log::error!("Failed to call is_same_hardware_id, {:?}", e);
                device_index += 1;
                continue;
            }
            _ => {}
        }

        // DIF_REMOVE 설치 함수 매개변수를 설정합니다.
        let mut remove_device_params = SP_REMOVEDEVICE_PARAMS {
            ClassInstallHeader: SP_CLASSINSTALL_HEADER {
                cbSize: std::mem::size_of::<SP_CLASSINSTALL_HEADER>() as _,
                InstallFunction: DIF_REMOVE,
            },
            Scope: DI_REMOVEDEVICE_GLOBAL,
            HwProfile: 0,
        };

        // 제거 명령 매개변수를 설정합니다.
        if SetupDiSetClassInstallParamsW(
            *dev_info,
            &mut devinfo_data,
            &mut remove_device_params.ClassInstallHeader,
            std::mem::size_of::<SP_REMOVEDEVICE_PARAMS>() as _,
        ) == FALSE
        {
            return Err(DeviceError::new_api_last_err(
                "SetupDiSetClassInstallParams",
            ));
        }

        // DIF_REMOVE 설치 함수를 호출하여 디바이스를 제거합니다.
        if SetupDiCallClassInstaller(DIF_REMOVE, *dev_info, &mut devinfo_data) == FALSE {
            return Err(DeviceError::new_api_last_err("SetupDiCallClassInstaller"));
        }

        // 디바이스 설치 매개변수를 조회하여 재부팅 필요 여부를 확인합니다.
        let mut device_params = SP_DEVINSTALL_PARAMS_W {
            cbSize: std::mem::size_of::<SP_DEVINSTALL_PARAMS_W>() as _,
            Flags: 0,
            FlagsEx: 0,
            hwndParent: null_mut(),
            InstallMsgHandler: None,
            InstallMsgHandlerContext: null_mut(),
            FileQueue: null_mut(),
            ClassInstallReserved: 0,
            Reserved: 0,
            DriverPath: [0; MAX_PATH],
        };

        // 디바이스 설치 플래그를 읽습니다.
        if SetupDiGetDeviceInstallParamsW(*dev_info, &mut devinfo_data, &mut device_params) == FALSE
        {
            log::error!(
                "Failed to call SetupDiGetDeviceInstallParamsW, {:?}",
                io::Error::last_os_error()
            );
        } else {
            // DI_NEEDRESTART 또는 DI_NEEDREBOOT 플래그가 설정되어 있으면 재부팅이 필요합니다.
            if device_params.Flags & (DI_NEEDRESTART | DI_NEEDREBOOT) != 0 {
                *reboot_required = true;
            }
        }

        device_index += 1;
    }

    Ok(())
}

// 디바이스에 대해 I/O 제어 작업을 수행합니다.
// 주어진 인터페이스 GUID를 가진 디바이스의 핸들을 열고 DeviceIoControl을 호출합니다.
// 입력 버퍼와 출력 버퍼의 최대 길이를 지정하여 데이터를 송수신합니다.
pub unsafe fn device_io_control(
    interface_guid: &GUID,
    control_code: u32,
    inbuf: &[u8],
    outbuf_max_len: usize,
) -> Result<Vec<u8>, DeviceError> {
    // 인터페이스 GUID를 가진 디바이스 핸들을 엽니다.
    let h_device = open_device_handle(interface_guid)?;
    let mut bytes_returned = 0;
    let mut outbuf: Vec<u8> = vec![];

    // 출력 버퍼를 준비합니다. 최대 길이가 0이 아닌 경우 메모리를 할당합니다.
    let outbuf_ptr = if outbuf_max_len > 0 {
        outbuf.reserve(outbuf_max_len);
        outbuf.as_mut_ptr()
    } else {
        null_mut()
    };

    // Windows API를 통해 디바이스 I/O 제어 명령을 실행합니다.
    // 입력 데이터, 제어 코드, 출력 버퍼를 전달합니다.
    let result = DeviceIoControl(
        h_device,
        control_code,
        inbuf.as_ptr() as _,
        inbuf.len() as _,
        outbuf_ptr as _,
        outbuf_max_len as _,
        &mut bytes_returned,
        null_mut(),
    );
    // 디바이스 핸들을 닫습니다.
    CloseHandle(h_device);

    if result == FALSE {
        return Err(DeviceError::new_api_last_err("DeviceIoControl"));
    }

    // 출력 버퍼의 실제 데이터 길이를 설정합니다.
    if outbuf_max_len > 0 {
        outbuf.set_len(bytes_returned as _);
        Ok(outbuf)
    } else {
        Ok(Vec::new())
    }
}

// 주어진 인터페이스 GUID를 가진 디바이스의 경로를 조회합니다.
// 디바이스 경로는 CreateFileW를 통해 디바이스를 열기 위해 필요합니다.
// 단계:
// 1. 해당 인터페이스 GUID를 가진 디바이스 정보를 조회합니다.
// 2. 첫 번째 디바이스 인터페이스를 열거합니다.
// 3. 디바이스 인터페이스 상세 정보에서 경로 문자열을 추출합니다.
unsafe fn get_device_path(interface_guid: &GUID) -> Result<Vec<u16>, DeviceError> {
    // 주어진 인터페이스 GUID를 가진 모든 디바이스를 조회합니다.
    let dev_info = DeviceInfo::setup_di_get_class_devs_ex_w(
        interface_guid,
        DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
    )?;

    // 디바이스 인터페이스 데이터 구조체를 초기화합니다.
    let mut device_interface_data = SP_DEVICE_INTERFACE_DATA {
        cbSize: std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as _,
        InterfaceClassGuid: *interface_guid,
        Flags: 0,
        Reserved: 0,
    };

    // 첫 번째(인덱스 0) 디바이스 인터페이스를 열거합니다.
    if SetupDiEnumDeviceInterfaces(
        *dev_info,
        null_mut(),
        interface_guid,
        0,
        &mut device_interface_data,
    ) == FALSE
    {
        return Err(DeviceError::new_api_last_err("SetupDiEnumDeviceInterfaces"));
    }

    // 디바이스 인터페이스 상세 정보의 필요한 길이를 먼저 조회합니다.
    let mut required_length = 0;
    if SetupDiGetDeviceInterfaceDetailW(
        *dev_info,
        &mut device_interface_data,
        null_mut(),
        0,
        &mut required_length,
        null_mut(),
    ) == FALSE
    {
        let err = io::Error::last_os_error();
        // ERROR_INSUFFICIENT_BUFFER는 버퍼 크기가 필요하다는 정상적인 오류입니다.
        if err.raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as _) {
            return Err(DeviceError::WinApiLastErr(
                "SetupDiGetDeviceInterfaceDetailW".to_string(),
                err,
            ));
        }
    }

    // 필요한 크기만큼 버퍼를 할당합니다.
    let predicted_length = required_length;
    let mut vec_data: Vec<u8> = Vec::with_capacity(required_length as _);
    let device_interface_detail_data = vec_data.as_mut_ptr();
    let device_interface_detail_data =
        device_interface_detail_data as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
    // 구조체의 크기를 설정합니다. (Windows API 요구사항)
    (*device_interface_detail_data).cbSize =
        std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as _;

    // 이제 실제 디바이스 인터페이스 상세 정보를 조회합니다.
    if SetupDiGetDeviceInterfaceDetailW(
        *dev_info,
        &mut device_interface_data,
        device_interface_detail_data,
        predicted_length,
        &mut required_length,
        null_mut(),
    ) == FALSE
    {
        return Err(DeviceError::new_api_last_err(
            "SetupDiGetDeviceInterfaceDetailW",
        ));
    }

    // 디바이스 경로 문자열(UTF-16)을 추출합니다.
    let mut path = Vec::new();
    let device_path_ptr =
        std::ptr::addr_of!((*device_interface_detail_data).DevicePath) as *const u16;
    // DevicePath 필드까지의 오프셋을 계산합니다.
    let steps = device_path_ptr as usize - vec_data.as_ptr() as usize;
    // 경로 문자열을 한 글자씩 복사합니다. (null 종료자 포함)
    for i in 0..(predicted_length - steps as u32) / 2 {
        if *device_path_ptr.offset(i as _) == 0 {
            path.push(0);
            break;
        }
        path.push(*device_path_ptr.offset(i as _));
    }
    Ok(path)
}

// 주어진 인터페이스 GUID를 가진 디바이스를 열고 Windows 파일 핸들을 반환합니다.
// 이 핸들은 DeviceIoControl을 통해 I/O 제어 명령을 전송하는 데 사용됩니다.
unsafe fn open_device_handle(interface_guid: &GUID) -> Result<HANDLE, DeviceError> {
    // 디바이스의 경로를 먼저 조회합니다.
    let device_path = get_device_path(interface_guid)?;
    // CreateFileW를 호출하여 디바이스 파일을 열고 핸들을 얻습니다.
    // GENERIC_READ | GENERIC_WRITE 플래그를 사용하여 읽기/쓰기 권한을 요청합니다.
    let h_device = CreateFileW(
        device_path.as_ptr(),
        GENERIC_READ | GENERIC_WRITE,
        0,
        null_mut(),
        OPEN_EXISTING,
        0,
        null_mut(),
    );
    // 핸들 검증: INVALID_HANDLE_VALUE 또는 NULL이면 오류입니다.
    if h_device == INVALID_HANDLE_VALUE || h_device == NULL {
        return Err(DeviceError::new_api_last_err("CreateFileW"));
    }
    Ok(h_device)
}
