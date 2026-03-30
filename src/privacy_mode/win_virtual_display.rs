use super::{PrivacyMode, PrivacyModeState, INVALID_PRIVACY_MODE_CONN_ID, NO_PHYSICAL_DISPLAYS};
use crate::{platform::windows::reg_display_settings, virtual_display_manager};
use hbb_common::{allow_err, bail, config::Config, log, ResultType};
use std::{
    io::Error,
    ops::{Deref, DerefMut},
    thread,
    time::Duration,
};
use virtual_display::MonitorMode;
use winapi::{
    shared::{
        minwindef::{DWORD, FALSE},
        ntdef::{NULL, WCHAR},
    },
    um::{
        wingdi::{
            DEVMODEW, DISPLAY_DEVICEW, DISPLAY_DEVICE_ACTIVE, DISPLAY_DEVICE_ATTACHED_TO_DESKTOP,
            DISPLAY_DEVICE_MIRRORING_DRIVER, DISPLAY_DEVICE_PRIMARY_DEVICE, DM_POSITION,
        },
        winuser::{
            ChangeDisplaySettingsExW, EnumDisplayDevicesW, EnumDisplaySettingsExW,
            EnumDisplaySettingsW, CDS_NORESET, CDS_RESET, CDS_SET_PRIMARY, CDS_UPDATEREGISTRY,
            DISP_CHANGE_FAILED, DISP_CHANGE_SUCCESSFUL, EDD_GET_DEVICE_INTERFACE_NAME,
            ENUM_CURRENT_SETTINGS, ENUM_REGISTRY_SETTINGS,
        },
    },
};

// 가상 디스플레이를 사용하는 프라이버시 모드 구현 식별자
pub(super) const PRIVACY_MODE_IMPL: &str = super::PRIVACY_MODE_IMPL_WIN_VIRTUAL_DISPLAY;

// 레지스트리 복구 정보를 저장하는 설정 키
const CONFIG_KEY_REG_RECOVERY: &str = "reg_recovery";

/// Windows 디스플레이 설정 정보를 저장하는 구조체입니다.
struct Display {
    /// 디스플레이 모드 정보 (해상도, 주사율 등)
    dm: DEVMODEW,
    /// 디스플레이 장치명 (예: "\\.\DISPLAY1")
    name: [WCHAR; 32],
    /// 이 디스플레이가 주 모니터인지 여부
    primary: bool,
}

/// 가상 디스플레이를 사용하는 프라이버시 모드 구현입니다.
///
/// 물리 디스플레이를 비활성화하고 가상 디스플레이로 대체하여
/// 사용자의 화면을 보호합니다. 프라이버시 모드 종료 시
/// 원래 디스플레이 설정을 복원합니다.
pub struct PrivacyModeImpl {
    /// 프라이버시 모드 구현 식별자
    impl_key: String,
    /// 현재 활성화된 프라이버시 모드의 연결 ID
    conn_id: i32,
    /// 프라이버시 모드 활성화 전의 물리 디스플레이 설정
    displays: Vec<Display>,
    /// 프라이버시 모드 활성화 전의 가상 디스플레이 설정
    virtual_displays: Vec<Display>,
    /// 프라이버시 모드 중에 추가된 가상 디스플레이 ID 목록
    virtual_displays_added: Vec<u32>,
}

/// 프라이버시 모드 활성화 과정의 실패 시 자동 롤백을 담당하는 RAII 가드입니다.
///
/// 프라이버시 모드 활성화 중에 오류가 발생하면 (succeeded = false)
/// 드롭 시 자동으로 프라이버시 모드를 비활성화하여 일관성 있는 상태를 유지합니다.
struct TurnOnGuard<'a> {
    /// 프라이버시 모드 구현 인스턴스
    privacy_mode: &'a mut PrivacyModeImpl,
    /// 활성화 성공 여부 (false면 롤백 수행)
    succeeded: bool,
}

impl<'a> Deref for TurnOnGuard<'a> {
    type Target = PrivacyModeImpl;

    fn deref(&self) -> &Self::Target {
        self.privacy_mode
    }
}

impl<'a> DerefMut for TurnOnGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.privacy_mode
    }
}

impl<'a> Drop for TurnOnGuard<'a> {
    /// 활성화 실패 시 프라이버시 모드를 비활성화합니다.
    fn drop(&mut self) {
        if !self.succeeded {
            self.privacy_mode
                .turn_off_privacy(INVALID_PRIVACY_MODE_CONN_ID, None)
                .ok();
        }
    }
}

impl PrivacyModeImpl {
    /// 새로운 프라이버시 모드 구현 인스턴스를 생성합니다.
    pub fn new(impl_key: &str) -> Self {
        Self {
            impl_key: impl_key.to_owned(),
            conn_id: INVALID_PRIVACY_MODE_CONN_ID,
            displays: Vec::new(),
            virtual_displays: Vec::new(),
            virtual_displays_added: Vec::new(),
        }
    }

    /// 현재 시스템의 모든 디스플레이 정보를 수집합니다.
    ///
    /// # 동작 과정
    /// 1. 시스템에 연결된 모든 디스플레이 열거
    /// 2. 활성화되고 미러링 드라이버가 아닌 디스플레이만 선택
    /// 3. 각 디스플레이의 현재 또는 레지스트리 설정 취득
    /// 4. 디스플레이를 가상 디스플레이와 물리 디스플레이로 분류
    ///
    /// 참고: https://github.com/rustdesk-org/rustdesk/blob/44c3a52ca8502cf53b58b59db130611778d34dbe/libs/scrap/src/dxgi/mod.rs#L365
    fn set_displays(&mut self) {
        self.displays.clear();
        self.virtual_displays.clear();

        let mut i: DWORD = 0;
        loop {
            #[allow(invalid_value)]
            let mut dd: DISPLAY_DEVICEW = unsafe { std::mem::MaybeUninit::uninit().assume_init() };
            dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as _;
            let ok = unsafe { EnumDisplayDevicesW(std::ptr::null(), i, &mut dd as _, 0) };
            if ok == FALSE {
                break;
            }
            i += 1;
            if 0 == (dd.StateFlags & DISPLAY_DEVICE_ACTIVE)
                || (dd.StateFlags & DISPLAY_DEVICE_MIRRORING_DRIVER) > 0
            {
                continue;
            }
            #[allow(invalid_value)]
            let mut dm: DEVMODEW = unsafe { std::mem::MaybeUninit::uninit().assume_init() };
            dm.dmSize = std::mem::size_of::<DEVMODEW>() as _;
            dm.dmDriverExtra = 0;
            unsafe {
                if FALSE
                    == EnumDisplaySettingsExW(
                        dd.DeviceName.as_ptr(),
                        ENUM_CURRENT_SETTINGS,
                        &mut dm as _,
                        0,
                    )
                {
                    if FALSE
                        == EnumDisplaySettingsExW(
                            dd.DeviceName.as_ptr(),
                            ENUM_REGISTRY_SETTINGS,
                            &mut dm as _,
                            0,
                        )
                    {
                        continue;
                    }
                }
            }

            let primary = (dd.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE) > 0;
            let display = Display {
                dm,
                name: dd.DeviceName,
                primary,
            };

            let ds = virtual_display_manager::get_cur_device_string();
            if let Ok(s) = String::from_utf16(&dd.DeviceString) {
                if s.len() >= ds.len() && &s[..ds.len()] == ds {
                    self.virtual_displays.push(display);
                    continue;
                }
            }
            self.displays.push(display);
        }
    }

    /// 추가된 가상 디스플레이를 제거합니다.
    fn restore_plug_out_monitor(&mut self) {
        let _ = virtual_display_manager::plug_out_monitor_indices(
            &self.virtual_displays_added,
            true,
            false,
        );
        self.virtual_displays_added.clear();
    }

    /// ChangeDisplaySettingsEx API 오류 코드를 사용자 친화적인 메시지로 변환합니다.
    #[inline]
    fn change_display_settings_ex_err_msg(rc: i32) -> String {
        if rc != DISP_CHANGE_FAILED {
            format!("ret: {}", rc)
        } else {
            format!(
                "ret: {}, last error: {:?}",
                rc,
                std::io::Error::last_os_error()
            )
        }
    }

    /// 첫 번째 가상 디스플레이를 주 모니터로 설정합니다.
    ///
    /// # 동작 과정
    /// 1. 첫 번째 가상 디스플레이의 현재 설정 취득
    /// 2. 위치를 (0, 0)으로 설정
    /// 3. 가상 디스플레이를 주 모니터로 설정
    /// 4. 기타 물리 디스플레이의 위치를 조정 (가상 디스플레이가 (0, 0)이므로)
    /// 5. 모든 디스플레이 설정 변경 사항을 윈도우 레지스트리에 저장
    ///
    /// # 반환값
    /// - Ok(display_name): 주 모니터로 설정된 가상 디스플레이의 이름
    /// - Err: 설정 변경 실패
    fn set_primary_display(&mut self) -> ResultType<String> {
        // 첫 번째 가상 디스플레이를 주 모니터로 설정
        let display = &self.virtual_displays[0];
        let display_name = std::string::String::from_utf16(&display.name)?;

        #[allow(invalid_value)]
        let mut new_primary_dm: DEVMODEW = unsafe { std::mem::MaybeUninit::uninit().assume_init() };
        new_primary_dm.dmSize = std::mem::size_of::<DEVMODEW>() as _;
        new_primary_dm.dmDriverExtra = 0;
        unsafe {
            // 가상 디스플레이의 현재 설정 취득
            if FALSE
                == EnumDisplaySettingsW(
                    display.name.as_ptr(),
                    ENUM_CURRENT_SETTINGS,
                    &mut new_primary_dm,
                )
            {
                bail!(
                    "Failed EnumDisplaySettingsW, device name: {:?}, error: {}",
                    std::string::String::from_utf16(&display.name),
                    Error::last_os_error()
                );
            }

            // Windows 24H2에서는 가상 디스플레이를 먼저 설정해야 함
            // 참고: https://developercommunity.visualstudio.com/t/Windows-11-Enterprise-24H2-using-WinApi/10851936?sort=newest
            let flags = CDS_UPDATEREGISTRY | CDS_NORESET;
            // 다른 디스플레이의 위치 조정을 위해 원래 위치 저장
            let offx = new_primary_dm.u1.s2().dmPosition.x;
            let offy = new_primary_dm.u1.s2().dmPosition.y;
            // 가상 디스플레이의 위치를 (0, 0)으로 설정
            new_primary_dm.u1.s2_mut().dmPosition.x = 0;
            new_primary_dm.u1.s2_mut().dmPosition.y = 0;
            new_primary_dm.dmFields |= DM_POSITION;
            // 가상 디스플레이를 주 모니터로 설정
            let rc = ChangeDisplaySettingsExW(
                display.name.as_ptr(),
                &mut new_primary_dm,
                NULL as _,
                flags | CDS_SET_PRIMARY,
                NULL,
            );
            if rc != DISP_CHANGE_SUCCESSFUL {
                let err = Self::change_display_settings_ex_err_msg(rc);
                log::error!(
                    "Failed ChangeDisplaySettingsEx, the virtual display, {}",
                    &err
                );
                bail!("Failed ChangeDisplaySettingsEx, {}", err);
            }

            // 다른 모든 물리 디스플레이의 위치 조정
            let mut i: DWORD = 0;
            loop {
                #[allow(invalid_value)]
                let mut dd: DISPLAY_DEVICEW = std::mem::MaybeUninit::uninit().assume_init();
                dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as _;
                if FALSE
                    == EnumDisplayDevicesW(NULL as _, i, &mut dd, EDD_GET_DEVICE_INTERFACE_NAME)
                {
                    break;
                }
                i += 1;
                // 데스크톱에 연결되지 않은 디스플레이는 무시
                if (dd.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP) == 0 {
                    continue;
                }
                // 가상 디스플레이는 건너뛰기
                if dd.DeviceName == display.name {
                    continue;
                }

                #[allow(invalid_value)]
                let mut dm: DEVMODEW = std::mem::MaybeUninit::uninit().assume_init();
                dm.dmSize = std::mem::size_of::<DEVMODEW>() as _;
                dm.dmDriverExtra = 0;
                if FALSE
                    == EnumDisplaySettingsW(dd.DeviceName.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm)
                {
                    bail!(
                        "Failed EnumDisplaySettingsW, device name: {:?}, error: {}",
                        std::string::String::from_utf16(&dd.DeviceName),
                        Error::last_os_error()
                    );
                }

                // 물리 디스플레이의 위치를 가상 디스플레이 기준으로 조정
                dm.u1.s2_mut().dmPosition.x -= offx;
                dm.u1.s2_mut().dmPosition.y -= offy;
                dm.dmFields |= DM_POSITION;
                // 조정된 설정 적용
                let rc = ChangeDisplaySettingsExW(
                    dd.DeviceName.as_ptr(),
                    &mut dm,
                    NULL as _,
                    flags,
                    NULL,
                );
                if rc != DISP_CHANGE_SUCCESSFUL {
                    let err = Self::change_display_settings_ex_err_msg(rc);
                    log::error!(
                        "Failed ChangeDisplaySettingsEx, device name: {:?}, flags: {}, {}",
                        std::string::String::from_utf16(&dd.DeviceName),
                        flags,
                        &err
                    );
                    bail!("Failed ChangeDisplaySettingsEx, {}", err);
                }

                // DPI 설정에 대한 참고 자료:
                // - https://stackoverflow.com/questions/35233182/how-can-i-change-windows-10-display-scaling-programmatically-using-c-sharp
                // - https://github.com/lihas/windows-DPI-scaling-sample/blob/master/DPIHelper/DpiHelper.cpp
                // 주의: Windows API에서는 DPI 취득/설정 공식 API를 제공하지 않습니다.
                // - https://learn.microsoft.com/en-us/windows/win32/api/wingdi/ne-wingdi-displayconfig_device_info_type
                // - https://github.com/lihas/windows-DPI-scaling-sample/blob/738ac18b7a7ce2d8fdc157eb825de9cb5eee0448/DPIHelper/DpiHelper.h#L37
            }
        }

        Ok(display_name)
    }

    /// 모든 물리 디스플레이를 비활성화합니다.
    ///
    /// 화면을 보호하기 위해 물리 디스플레이의 해상도를 0으로 설정하고
    /// 위치를 표시 범위 밖으로 이동합니다.
    ///
    /// 주의: Windows 24H2에서는 다른 가상 디스플레이를 감지할 수 없습니다.
    /// `DeviceString == virtual_display_manager::get_cur_device_string()` 비교로만
    /// 가상 디스플레이를 식별할 수 있으며, 프라이버시 모드 종료 후 복원이 불가능할 수 있습니다.
    fn disable_physical_displays(&self) -> ResultType<()> {
        // 모든 물리 디스플레이에 대해 처리
        for display in &self.displays {
            let mut dm = display.dm.clone();
            unsafe {
                // 위치를 화면 범위 밖으로 이동 (10000, 10000)
                dm.u1.s2_mut().dmPosition.x = 10000;
                dm.u1.s2_mut().dmPosition.y = 10000;
                // 해상도를 0으로 설정하여 비활성화
                dm.dmPelsHeight = 0;
                dm.dmPelsWidth = 0;
                let flags = CDS_UPDATEREGISTRY | CDS_NORESET;
                let rc = ChangeDisplaySettingsExW(
                    display.name.as_ptr(),
                    &mut dm,
                    NULL as _,
                    flags,
                    NULL as _,
                );
                if rc != DISP_CHANGE_SUCCESSFUL {
                    let err = Self::change_display_settings_ex_err_msg(rc);
                    log::error!(
                        "Failed ChangeDisplaySettingsEx, device name: {:?}, flags: {}, {}",
                        std::string::String::from_utf16(&display.name),
                        flags,
                        &err
                    );
                    bail!("Failed ChangeDisplaySettingsEx, {}", err);
                }
            }
        }
        Ok(())
    }

    /// 기본 가상 디스플레이 모드 (1920x1080 @ 60Hz)를 반환합니다.
    #[inline]
    fn default_display_modes() -> Vec<MonitorMode> {
        vec![MonitorMode {
            width: 1920,
            height: 1080,
            sync: 60,
        }]
    }

    /// 가상 디스플레이가 준비될 때까지 대기하고 확인합니다.
    ///
    /// # 동작 과정
    /// 1. 가상 디스플레이가 아직 추가되지 않으면 추가
    /// 2. 비동기 모드이면 1초 대기
    /// 3. 모든 디스플레이 정보 다시 수집
    /// 4. 물리 디스플레이 확인 (물리 디스플레이가 없으면 실패)
    /// 5. 비동기 모드이면 최대 5초 동안 가상 디스플레이 준비 대기
    ///
    /// # 인자
    /// - `is_async_mode`: 비동기 모드 여부
    ///
    /// # 반환값
    /// - Ok(()): 가상 디스플레이 준비 완료 또는 이미 추가됨
    /// - Err: 물리 디스플레이 없음 또는 다른 오류
    ///
    /// # 주의
    /// 최대 6초까지 대기할 수 있습니다. 이는 다음 이유로 수용됩니다:
    /// 1. 비동기 프라이버시 모드는 별도 스레드에서 처리됨
    /// 2. 사용자는 일반적으로 프라이버시 모드 활성화를 급하게 하지 않음
    pub fn ensure_virtual_display(&mut self, is_async_mode: bool) -> ResultType<()> {
        if self.virtual_displays.is_empty() {
            // 기본 가상 디스플레이 추가
            let displays =
                virtual_display_manager::plug_in_peer_request(vec![Self::default_display_modes()])?;
            if is_async_mode {
                thread::sleep(Duration::from_secs(1));
            }
            // 현재 디스플레이 정보 갱신
            self.set_displays();
            // 물리 디스플레이 확인
            if self.displays.is_empty() {
                virtual_display_manager::plug_out_monitor_indices(&displays, false, false)?;
                bail!(NO_PHYSICAL_DISPLAYS);
            }

            // 비동기 모드이면 가상 디스플레이 준비 대기 (최대 5초)
            if is_async_mode {
                let now = std::time::Instant::now();
                while self.virtual_displays.is_empty()
                    && now.elapsed() < Duration::from_millis(5000)
                {
                    thread::sleep(Duration::from_millis(500));
                    self.set_displays();
                }
            }

            self.virtual_displays_added.extend(displays);
        }

        Ok(())
    }

    /// 변경된 디스플레이 설정을 적용합니다.
    ///
    /// # 인자
    /// - `flags`: 디스플레이 설정 플래그 (CDS_RESET 또는 CDS_NORESET)
    ///
    /// # 반환값
    /// - Ok(()): 디스플레이 설정 적용 성공
    /// - Err: 적용 실패
    #[inline]
    fn commit_change_display(flags: DWORD) -> ResultType<()> {
        unsafe {
            // 주석 처리된 코드: 데스크톱 전환 관련 (현재 필요 없음)
            // let rc = ChangeDisplaySettingsExW(NULL as _, NULL as _, NULL as _, flags, NULL as _);

            let rc = ChangeDisplaySettingsExW(NULL as _, NULL as _, NULL as _, flags, NULL as _);
            if rc != DISP_CHANGE_SUCCESSFUL {
                let err = Self::change_display_settings_ex_err_msg(rc);
                bail!("Failed ChangeDisplaySettingsEx, {}", err);
            }
        }
        Ok(())
    }

    /// 모든 디스플레이 설정을 원래 상태로 복원합니다.
    fn restore(&mut self) {
        // 물리 및 가상 디스플레이 설정 복원
        Self::restore_displays(&self.displays);
        Self::restore_displays(&self.virtual_displays);
        allow_err!(Self::commit_change_display(0));
        self.displays.clear();
        self.virtual_displays.clear();
        let is_virtual_display_added = self.virtual_displays_added.len() > 0;
        if is_virtual_display_added {
            // 추가된 가상 디스플레이 제거
            self.restore_plug_out_monitor();
        } else {
            // 가상 디스플레이가 추가되지 않은 경우
            // 참고: https://github.com/rustdesk/rustdesk/pull/12114#issuecomment-2983054370
            // 디스플레이 설정을 강제 리로드하기 위해 가상 디스플레이 조합 변경
            // 이는 프라이버시 모드의 안정성을 높입니다.
            // 가상 디스플레이 제거 후 복구할 필요는 없습니다.
            let _ = virtual_display_manager::plug_out_monitor(-1, true, false);

            // TODO: IDD_IMPL_AMYUNI 가상 디스플레이를 짧은 시간에 제거+재추가하면
            // 서버 측이 충돌하므로 현재 여기서 재추가하지 않습니다.
        }
    }

    /// 디스플레이 목록의 설정을 원래 상태로 복원합니다.
    fn restore_displays(displays: &[Display]) {
        for display in displays {
            unsafe {
                let mut dm = display.dm.clone();
                // 주 모니터 설정 여부에 따라 플래그 구분
                let flags = if display.primary {
                    CDS_NORESET | CDS_UPDATEREGISTRY | CDS_SET_PRIMARY
                } else {
                    CDS_NORESET | CDS_UPDATEREGISTRY
                };
                ChangeDisplaySettingsExW(
                    display.name.as_ptr(),
                    &mut dm,
                    std::ptr::null_mut(),
                    flags,
                    std::ptr::null_mut(),
                );
            }
        }
    }
}

impl PrivacyMode for PrivacyModeImpl {
    /// Amyuni IDD를 사용하면 비동기 모드입니다.
    fn is_async_privacy_mode(&self) -> bool {
        virtual_display_manager::is_amyuni_idd()
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
    /// 1. 가상 디스플레이 지원 여부 확인
    /// 2. 연결 ID 검증 (이미 활성화되었으면 반환)
    /// 3. 현재 디스플레이 설정 수집
    /// 4. 물리 디스플레이 확인 (없으면 실패)
    /// 5. 비동기 모드 여부 판정
    /// 6. TurnOnGuard를 사용한 안전한 활성화
    /// 7. 가상 디스플레이 준비 (없으면 실패)
    /// 8. 레지스트리 연결성 정보 읽기 (이전)
    /// 9. 첫 번째 가상 디스플레이를 주 모니터로 설정
    /// 10. 물리 디스플레이 비활성화
    /// 11. 디스플레이 설정 변경 적용 (CDS_RESET)
    /// 12. 가상 디스플레이 해상도를 1920x1080으로 설정
    /// 13. 레지스트리 연결성 정보 읽기 (이후) 및 복구 정보 저장
    /// 14. 입력 훅 설치
    /// 15. 활성화 성공 표시
    ///
    /// # 인자
    /// - `conn_id`: 연결 ID
    ///
    /// # 반환값
    /// - Ok(true): 프라이버시 모드 활성화 성공
    /// - Ok(false): 가상 디스플레이 미지원
    /// - Err: 실패
    fn turn_on_privacy(&mut self, conn_id: i32) -> ResultType<bool> {
        // 가상 디스플레이 지원 확인
        if !virtual_display_manager::is_virtual_display_supported() {
            bail!("idd_not_support_under_win10_2004_tip");
        }

        // 이미 활성화되었는지 확인
        if self.check_on_conn_id(conn_id)? {
            log::debug!("Privacy mode of conn {} is already on", conn_id);
            return Ok(true);
        }
        // 현재 디스플레이 설정 수집
        self.set_displays();
        // 물리 디스플레이 확인
        if self.displays.is_empty() {
            log::debug!("{}", NO_PHYSICAL_DISPLAYS);
            bail!(NO_PHYSICAL_DISPLAYS);
        }

        // 비동기 모드 여부 판정
        let is_async_mode = self.is_async_privacy_mode();
        let mut guard = TurnOnGuard {
            privacy_mode: self,
            succeeded: false,
        };

        // 가상 디스플레이 준비
        guard.ensure_virtual_display(is_async_mode)?;
        if guard.virtual_displays.is_empty() {
            log::debug!("No virtual displays");
            bail!("No virtual displays.");
        }

        // 레지스트리 연결성 정보 수집 (이전 상태)
        let reg_connectivity_1 = reg_display_settings::read_reg_connectivity()?;
        // 가상 디스플레이를 주 모니터로 설정
        let primary_display_name = guard.set_primary_display()?;
        // 물리 디스플레이 비활성화
        guard.disable_physical_displays()?;
        // 디스플레이 설정 변경 사항 적용
        Self::commit_change_display(CDS_RESET)?;
        // 가상 디스플레이 해상도 명시적 설정 (1920x1080)
        allow_err!(crate::platform::change_resolution(
            &primary_display_name,
            1920,
            1080
        ));
        // 레지스트리 연결성 정보 수집 (이후 상태)
        let reg_connectivity_2 = reg_display_settings::read_reg_connectivity()?;

        // 변경된 레지스트리 연결성 정보 저장
        if let Some(reg_recovery) =
            reg_display_settings::diff_recent_connectivity(reg_connectivity_1, reg_connectivity_2)
        {
            Config::set_option(
                CONFIG_KEY_REG_RECOVERY.to_owned(),
                serde_json::to_string(&reg_recovery)?,
            );
        } else {
            reset_config_reg_connectivity();
        };

        // 입력 훅 설치
        guard.conn_id = conn_id;
        guard.succeeded = true;

        allow_err!(super::win_input::hook());

        Ok(true)
    }

    /// 프라이버시 모드를 비활성화합니다.
    ///
    /// # 동작 과정
    /// 1. 연결 ID 검증
    /// 2. 입력 훅 제거
    /// 3. 임시 디스플레이 변경 무시 처리기 생성
    /// 4. 디스플레이 설정 복원
    /// 5. 레지스트리 연결성 복구 강제 실행
    /// 6. 상태 변화 알림 (필요시)
    ///
    /// # 인자
    /// - `conn_id`: 연결 ID
    /// - `state`: 프라이버시 모드 종료 상태 (선택사항)
    fn turn_off_privacy(
        &mut self,
        conn_id: i32,
        state: Option<PrivacyModeState>,
    ) -> ResultType<()> {
        // 연결 ID 검증
        self.check_off_conn_id(conn_id)?;
        // 입력 훅 제거
        super::win_input::unhook()?;
        // 임시로 디스플레이 변경 알림 무시
        let _tmp_ignore_changed_holder = crate::display_service::temp_ignore_displays_changed();
        // 디스플레이 설정 복원
        self.restore();
        // 레지스트리 연결성 강제 복구
        // restore()에서 변경된 레지스트리 연결성이 완전히 복구되지 않을 수 있으므로
        // 강제로 다시 복구합니다.
        restore_reg_connectivity(false, true);

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

impl Drop for PrivacyModeImpl {
    /// 인스턴스 소멸 시 프라이버시 모드를 비활성화합니다.
    fn drop(&mut self) {
        if self.conn_id != INVALID_PRIVACY_MODE_CONN_ID {
            allow_err!(self.turn_off_privacy(self.conn_id, None));
        }
    }
}

/// 레지스트리 복구 정보를 초기화합니다.
#[inline]
fn reset_config_reg_connectivity() {
    Config::set_option(CONFIG_KEY_REG_RECOVERY.to_owned(), "".to_owned());
}

/// 저장된 레지스트리 연결성 정보로부터 디스플레이 설정을 복구합니다.
///
/// # 동작 과정
/// 1. 저장된 레지스트리 복구 정보 읽기
/// 2. 정보가 비어있으면 반환
/// 3. 필요시 모든 가상 디스플레이 제거
/// 4. JSON 형식의 복구 정보 파싱
/// 5. 레지스트리 연결성 복구 실행
/// 6. 복구 정보 초기화
///
/// # 인자
/// - `plug_out_monitors`: 복구 전에 모든 가상 디스플레이를 제거할지 여부
/// - `force`: 강제 복구 여부
pub fn restore_reg_connectivity(plug_out_monitors: bool, force: bool) {
    let config_recovery_value = Config::get_option(CONFIG_KEY_REG_RECOVERY);
    if config_recovery_value.is_empty() {
        return;
    }
    // 필요시 모든 가상 디스플레이 제거
    if plug_out_monitors {
        let _ = virtual_display_manager::plug_out_monitor(-1, true, false);
    }
    // JSON 파싱 및 레지스트리 연결성 복구
    if let Ok(reg_recovery) =
        serde_json::from_str::<reg_display_settings::RegRecovery>(&config_recovery_value)
    {
        if let Err(e) = reg_display_settings::restore_reg_connectivity(reg_recovery, force) {
            log::error!("Failed restore_reg_connectivity, error: {}", e);
        }
    }
    // 복구 정보 초기화
    reset_config_reg_connectivity();
}
