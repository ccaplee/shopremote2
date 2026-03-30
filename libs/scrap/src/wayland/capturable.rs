use std::boxed::Box;
use std::error::Error;

/// 픽셀 데이터 제공자 - 다양한 색상 형식을 지원합니다.
pub enum PixelProvider<'a> {
    /// RGB 형식 (각 색상 8비트, 24비트)
    RGB(usize, usize, &'a [u8]),
    /// RGB + 1바이트 패딩 형식 (32비트)
    RGB0(usize, usize, &'a [u8]),
    /// BGR + 1바이트 패딩 형식 (32비트, 널리 사용)
    BGR0(usize, usize, &'a [u8]),
    /// BGR + 패딩 형식 (width, height, stride를 명시적으로 지정)
    BGR0S(usize, usize, usize, &'a [u8]),
    /// 데이터 없음
    NONE,
}

impl<'a> PixelProvider<'a> {
    pub fn size(&self) -> (usize, usize) {
        match self {
            PixelProvider::RGB(w, h, _) => (*w, *h),
            PixelProvider::RGB0(w, h, _) => (*w, *h),
            PixelProvider::BGR0(w, h, _) => (*w, *h),
            PixelProvider::BGR0S(w, h, _, _) => (*w, *h),
            PixelProvider::NONE => (0, 0),
        }
    }
}

pub trait Recorder {
    fn capture(&mut self, timeout_ms: u64) -> Result<PixelProvider, Box<dyn Error>>;
}

pub trait BoxCloneCapturable {
    fn box_clone(&self) -> Box<dyn Capturable>;
}

impl<T> BoxCloneCapturable for T
where
    T: Clone + Capturable + 'static,
{
    fn box_clone(&self) -> Box<dyn Capturable> {
        Box::new(self.clone())
    }
}

/// 캡처 가능한 리소스 (창, 디스플레이 등)의 특성
pub trait Capturable: Send + BoxCloneCapturable {
    /// 캡처 가능 리소스의 이름 (창의 경우 제목 표시줄 텍스트 등)
    fn name(&self) -> String;

    /// 캡처 가능 리소스의 기하학적 정보를 화면 크기 대비 상대값으로 반환합니다.
    /// 반환값: (x, y, width, height) - 0.0 ~ 1.0 범위
    /// 예: x=0.5, y=0.0, width=0.5, height=1.0 = 화면의 오른쪽 절반
    fn geometry_relative(&self) -> Result<(f64, f64, f64, f64), Box<dyn Error>>;

    /// 입력 이벤트 시뮬레이션 직전에 호출되는 콜백
    /// 입력 전에 창을 포커스하는 등의 준비 작업에 유용합니다.
    fn before_input(&mut self) -> Result<(), Box<dyn Error>>;

    /// 현재 캡처 가능 리소스를 기록할 수 있는 Recorder를 반환합니다.
    fn recorder(&self, capture_cursor: bool) -> Result<Box<dyn Recorder>, Box<dyn Error>>;
}

impl Clone for Box<dyn Capturable> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}
