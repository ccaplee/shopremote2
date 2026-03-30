// VPX 인코더/디코더 구현 관련 참고 자료
// https://github.com/astraw/vpx-encode
// https://github.com/astraw/env-libvpx-sys
// https://github.com/rust-av/vpx-rs/blob/master/src/decoder.rs
// https://github.com/chromium/chromium/blob/e7b24573bc2e06fed4749dd6b6abfce67f29052f/media/video/vpx_video_encoder.cc#L522

use hbb_common::anyhow::{anyhow, Context};
use hbb_common::log;
use hbb_common::message_proto::{Chroma, EncodedVideoFrame, EncodedVideoFrames, VideoFrame};
use hbb_common::ResultType;

use crate::codec::{base_bitrate, codec_thread_num, EncoderApi};
use crate::{EncodeInput, EncodeYuvFormat, GoogleImage, Pixfmt, STRIDE_ALIGN};

use super::vpx::{vp8e_enc_control_id::*, vpx_codec_err_t::*, *};
use crate::{generate_call_macro, generate_call_ptr_macro, Error, Result};
use hbb_common::bytes::Bytes;
use std::os::raw::{c_int, c_uint};
use std::{ptr, slice};

// VPX 함수 호출 매크로 생성
generate_call_macro!(call_vpx, false);
generate_call_ptr_macro!(call_vpx_ptr);

/// VP8/VP9 코덱 식별자
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum VpxVideoCodecId {
    VP8,  // VP8 코덱
    VP9,  // VP9 코덱
}

impl Default for VpxVideoCodecId {
    fn default() -> VpxVideoCodecId {
        VpxVideoCodecId::VP9
    }
}

/// VP8/VP9 비디오 인코더 구조체
pub struct VpxEncoder {
    ctx: vpx_codec_ctx_t,  // 인코더 컨텍스트
    width: usize,           // 영상 너비
    height: usize,          // 영상 높이
    id: VpxVideoCodecId,    // 사용 중인 코덱
    i444: bool,             // I444 형식 여부 (일반적으로 I420)
    yuvfmt: EncodeYuvFormat, // 인코더가 받는 YUV 형식
}

/// VP8/VP9 비디오 디코더 구조체
pub struct VpxDecoder {
    ctx: vpx_codec_ctx_t,   // 디코더 컨텍스트
}

impl EncoderApi for VpxEncoder {
    fn new(cfg: crate::codec::EncoderCfg, i444: bool) -> ResultType<Self>
    where
        Self: Sized,
    {
        match cfg {
            crate::codec::EncoderCfg::VPX(config) => {
                // VP8 또는 VP9 인터페이스 선택
                let i = match config.codec {
                    VpxVideoCodecId::VP8 => call_vpx_ptr!(vpx_codec_vp8_cx()),
                    VpxVideoCodecId::VP9 => call_vpx_ptr!(vpx_codec_vp9_cx()),
                };

                // 기본 인코더 설정 값 초기화
                let mut c = unsafe { std::mem::MaybeUninit::zeroed().assume_init() };
                call_vpx!(vpx_codec_enc_config_default(i, &mut c, 0));

                // 참고: https://www.webmproject.org/docs/encoder-parameters/
                // 기본값: c.rc_min_quantizer = 0, c.rc_max_quantizer = 63
                // 나중에 rc_resize_allowed 시도 가능

                // 기본 설정 적용
                c.g_w = config.width;
                c.g_h = config.height;
                c.g_timebase.num = 1;
                c.g_timebase.den = 1000; // 타임스탬프 정밀도 (밀리초)
                c.rc_undershoot_pct = 95;
                // 데이터 버퍼가 이 백분율 미만으로 떨어지면 프레임 드롭 표시
                // 0으로 설정하면 비활성화
                // 동적 장면에서 낮은 비트레이트는 낮은 FPS, 높은 비트레이트는 높은 FPS 제공
                c.rc_dropframe_thresh = 25;
                c.g_threads = codec_thread_num(64) as _;
                c.g_error_resilient = VPX_ERROR_RESILIENT_DEFAULT;

                // 참고: https://developers.google.com/media/vp9/bitrate-modes/
                // VP9의 실시간 스트리밍에는 CBR(상수 비트레이트) 모드 권장
                c.rc_end_usage = vpx_rc_mode::VPX_CBR;

                // 키프레임 간격 설정
                if let Some(keyframe_interval) = config.keyframe_interval {
                    c.kf_min_dist = 0;
                    c.kf_max_dist = keyframe_interval as _;
                } else {
                    c.kf_mode = vpx_kf_mode::VPX_KF_DISABLED; // 대역폭 대폭 감소
                }

                // 품질 설정에 따른 양자화 값 계산
                let (q_min, q_max) = Self::calc_q_values(config.quality);
                c.rc_min_quantizer = q_min;
                c.rc_max_quantizer = q_max;
                c.rc_target_bitrate =
                    Self::bitrate(config.width as _, config.height as _, config.quality);

                // 참고:
                // https://chromium.googlesource.com/webm/libvpx/+/refs/heads/main/vp9/common/vp9_enums.h#29
                // https://chromium.googlesource.com/webm/libvpx/+/refs/heads/main/vp8/vp8_cx_iface.c#282
                c.g_profile = if i444 && config.codec == VpxVideoCodecId::VP9 {
                    1  // VP9 프로필 1 (4:4:4)
                } else {
                    0  // 프로필 0 (4:2:0)
                };

                /*
                VPX 인코더는 비트율 제어 목적으로 2-패스 인코딩을 지원한다.
                2-패스 인코딩에서는 전체 인코딩 프로세스를 2번 수행한다.
                첫 번째 패스는 두 번째 패스를 위한 새로운 제어 매개변수를 생성한다.

                이 방식은 동일한 비트레이트에서 최고의 PSNR을 달성할 수 있게 한다.
                */

                // 인코더 컨텍스트 초기화
                let mut ctx = Default::default();
                call_vpx!(vpx_codec_enc_init_ver(
                    &mut ctx,
                    i,
                    &c,
                    0,
                    VPX_ENCODER_ABI_VERSION as _
                ));

                // VP9 특정 설정
                if config.codec == VpxVideoCodecId::VP9 {
                    // 인코더 내부 속도 설정
                    // ffmpeg에서는 --speed 옵션
                    /*
                    0 또는 양수 1-16으로 설정하면 코덱은 인코딩에 걸리는 시간에 따라
                    복잡도를 자동으로 조정하려고 시도한다.
                    이 숫자를 증가시키면 속도는 올라가고 품질은 내려간다.
                    음수는 엄격한 강제, 양수는 적응형이다.
                    */
                    /* https://developers.google.com/media/vp9/live-encoding
                    실시간/리얼타임 인코딩에는 속도 5~8을 사용해야 한다.
                    더 낮은 숫자(5~6)는 높은 품질이지만 CPU 전력이 더 필요하다.
                    더 높은 숫자(7~8)는 낮은 품질이지만 낮은 지연 시간과
                    저전력 기기(모바일 등)에 더 적합하다.
                    */
                    call_vpx!(vpx_codec_control_(&mut ctx, VP8E_SET_CPUUSED as _, 7,));

                    // 행 레벨 멀티스레딩 설정
                    /*
                    일부 사람들이 이미 언급했듯이, libvpx의 최신 버전은
                    -row-mt 1을 지원하여 타일 행 멀티스레딩을 활성화한다.
                    이는 VP9에서 타일 수를 최대 4배까지 증가시킬 수 있다
                    (비디오 높이에 관계없이 최대 타일 행 수는 4개).
                    이를 활성화하려면 -tile-rows N을 사용하는데,
                    N은 log2 단위의 타일 행 수이다
                    (-tile-rows 1은 2개 타일 행, -tile-rows 2는 4개 타일 행).
                    활성 스레드의 총 개수는 $tile_rows * $tile_columns과 같다.
                    */
                    call_vpx!(vpx_codec_control_(
                        &mut ctx,
                        VP9E_SET_ROW_MT as _,
                        1 as c_int
                    ));

                    call_vpx!(vpx_codec_control_(
                        &mut ctx,
                        VP9E_SET_TILE_COLUMNS as _,
                        4 as c_int
                    ));
                } else if config.codec == VpxVideoCodecId::VP8 {
                    // VP8 특정 설정
                    // https://github.com/webmproject/libvpx/blob/972149cafeb71d6f08df89e91a0130d6a38c4b15/vpx/vp8cx.h#L172
                    // https://groups.google.com/a/webmproject.org/g/webm-discuss/c/DJhSrmfQ61M
                    call_vpx!(vpx_codec_control_(&mut ctx, VP8E_SET_CPUUSED as _, 12,));
                }

                Ok(Self {
                    ctx,
                    width: config.width as _,
                    height: config.height as _,
                    id: config.codec,
                    i444,
                    yuvfmt: Self::get_yuvfmt(config.width, config.height, i444),
                })
            }
            _ => Err(anyhow!("encoder type mismatch")),
        }
    }

    fn encode_to_message(&mut self, input: EncodeInput, ms: i64) -> ResultType<VideoFrame> {
        let mut frames = Vec::new();

        // 입력 데이터를 인코딩하여 프레임 수집
        for ref frame in self
            .encode(ms, input.yuv()?, STRIDE_ALIGN)
            .with_context(|| "Failed to encode")?
        {
            frames.push(VpxEncoder::create_frame(frame));
        }

        // 버퍼에 남아있는 프레임도 반환
        for ref frame in self.flush().with_context(|| "Failed to flush")? {
            frames.push(VpxEncoder::create_frame(frame));
        }

        // TODO: 1초 간격으로 주기적으로 플러시
        if frames.len() > 0 {
            Ok(VpxEncoder::create_video_frame(self.id, frames))
        } else {
            Err(anyhow!("no valid frame"))
        }
    }

    fn yuvfmt(&self) -> crate::EncodeYuvFormat {
        self.yuvfmt.clone()
    }

    #[cfg(feature = "vram")]
    fn input_texture(&self) -> bool {
        false
    }

    fn set_quality(&mut self, ratio: f32) -> ResultType<()> {
        let mut c = unsafe { *self.ctx.config.enc.to_owned() };
        let (q_min, q_max) = Self::calc_q_values(ratio);
        c.rc_min_quantizer = q_min;
        c.rc_max_quantizer = q_max;
        c.rc_target_bitrate = Self::bitrate(self.width as _, self.height as _, ratio);
        call_vpx!(vpx_codec_enc_config_set(&mut self.ctx, &c));
        Ok(())
    }

    fn bitrate(&self) -> u32 {
        let c = unsafe { *self.ctx.config.enc.to_owned() };
        c.rc_target_bitrate
    }

    fn support_changing_quality(&self) -> bool {
        true
    }

    fn latency_free(&self) -> bool {
        true
    }

    fn is_hardware(&self) -> bool {
        false
    }

    fn disable(&self) {}
}

impl VpxEncoder {
    /// 영상 데이터를 VP8/VP9로 인코딩
    ///
    /// # 인자
    /// - `pts`: 프레젠테이션 타임스탬프
    /// - `data`: 인코딩할 YUV 데이터
    /// - `stride_align`: 스트라이드 정렬 크기
    pub fn encode<'a>(&'a mut self, pts: i64, data: &[u8], stride_align: usize) -> Result<EncodeFrames<'a>> {
        let bpp = if self.i444 { 24 } else { 12 };
        // 데이터 크기 검증
        if data.len() < self.width * self.height * bpp / 8 {
            return Err(Error::FailedCall("len not enough".to_string()));
        }

        // 픽셀 형식 선택
        let fmt = if self.i444 {
            vpx_img_fmt::VPX_IMG_FMT_I444
        } else {
            vpx_img_fmt::VPX_IMG_FMT_I420
        };

        // 이미지 래퍼 설정
        let mut image = Default::default();
        call_vpx_ptr!(vpx_img_wrap(
            &mut image,
            fmt,
            self.width as _,
            self.height as _,
            stride_align as _,
            data.as_ptr() as _,
        ));

        // 인코딩 수행
        call_vpx!(vpx_codec_encode(
            &mut self.ctx,
            &image,
            pts as _,
            1, // 지속 시간
            0, // 플래그
            VPX_DL_REALTIME as _,
        ));

        Ok(EncodeFrames {
            ctx: &mut self.ctx,
            iter: ptr::null(),
        })
    }

    /// 대기 중인 패킷을 인코더에서 반환하도록 요청
    pub fn flush<'a>(&'a mut self) -> Result<EncodeFrames<'a>> {
        call_vpx!(vpx_codec_encode(
            &mut self.ctx,
            ptr::null(),
            -1, // PTS
            1,  // 지속 시간
            0,  // 플래그
            VPX_DL_REALTIME as _,
        ));

        Ok(EncodeFrames {
            ctx: &mut self.ctx,
            iter: ptr::null(),
        })
    }

    /// 비디오 프레임을 프로토콜 메시지로 변환
    #[inline]
    pub fn create_video_frame(
        codec_id: VpxVideoCodecId,
        frames: Vec<EncodedVideoFrame>,
    ) -> VideoFrame {
        let mut vf = VideoFrame::new();
        let vpxs = EncodedVideoFrames {
            frames: frames.into(),
            ..Default::default()
        };
        // 코덱 타입에 따라 메시지 설정
        match codec_id {
            VpxVideoCodecId::VP8 => vf.set_vp8s(vpxs),
            VpxVideoCodecId::VP9 => vf.set_vp9s(vpxs),
        }
        vf
    }

    /// 인코딩된 프레임을 프로토콜 메시지로 변환
    #[inline]
    fn create_frame(frame: &EncodeFrame) -> EncodedVideoFrame {
        EncodedVideoFrame {
            data: Bytes::from(frame.data.to_vec()),
            key: frame.key,
            pts: frame.pts,
            ..Default::default()
        }
    }

    /// 비트레이트 계산
    fn bitrate(width: u32, height: u32, ratio: f32) -> u32 {
        let bitrate = base_bitrate(width, height) as f32;
        (bitrate * ratio) as u32
    }

    /// 품질 비율에 따른 양자화 값 계산
    /// 품질이 높을수록(ratio 1.0에 가까울수록) q_min이 낮아짐
    #[inline]
    fn calc_q_values(ratio: f32) -> (u32, u32) {
        let b = (ratio * 100.0) as u32;
        let b = std::cmp::min(b, 200);
        let q_min1 = 36;
        let q_min2 = 0;
        let q_max1 = 56;
        let q_max2 = 37;

        let t = b as f32 / 200.0;

        let mut q_min: u32 = ((1.0 - t) * q_min1 as f32 + t * q_min2 as f32).round() as u32;
        let mut q_max = ((1.0 - t) * q_max1 as f32 + t * q_max2 as f32).round() as u32;

        q_min = q_min.clamp(q_min2, q_min1);
        q_max = q_max.clamp(q_max2, q_max1);

        (q_min, q_max)
    }

    /// 인코더가 받는 YUV 형식 정보 획득
    fn get_yuvfmt(width: u32, height: u32, i444: bool) -> EncodeYuvFormat {
        let mut img = Default::default();
        let fmt = if i444 {
            vpx_img_fmt::VPX_IMG_FMT_I444
        } else {
            vpx_img_fmt::VPX_IMG_FMT_I420
        };
        unsafe {
            vpx_img_wrap(
                &mut img,
                fmt,
                width as _,
                height as _,
                crate::STRIDE_ALIGN as _,
                0x1 as _,
            );
        }
        let pixfmt = if i444 { Pixfmt::I444 } else { Pixfmt::I420 };
        EncodeYuvFormat {
            pixfmt,
            w: img.w as _,
            h: img.h as _,
            stride: img.stride.map(|s| s as usize).to_vec(),
            u: img.planes[1] as usize - img.planes[0] as usize,
            v: img.planes[2] as usize - img.planes[0] as usize,
        }
    }
}

impl Drop for VpxEncoder {
    fn drop(&mut self) {
        unsafe {
            let result = vpx_codec_destroy(&mut self.ctx);
            if result != VPX_CODEC_OK {
                panic!("failed to destroy vpx codec");
            }
        }
    }
}

/// 인코딩된 프레임 정보
#[derive(Clone, Copy, Debug)]
pub struct EncodeFrame<'a> {
    /// 압축된 데이터
    pub data: &'a [u8],
    /// 프레임이 키프레임인지 여부
    pub key: bool,
    /// 프레젠테이션 타임스탐프 (타임베이스 단위)
    pub pts: i64,
}

/// VP8/VP9 인코더 설정
#[derive(Clone, Copy, Debug)]
pub struct VpxEncoderConfig {
    /// 너비 (픽셀 단위)
    pub width: c_uint,
    /// 높이 (픽셀 단위)
    pub height: c_uint,
    /// 비트레이트 비율 (0.0 ~ 1.0)
    pub quality: f32,
    /// 사용할 코덱
    pub codec: VpxVideoCodecId,
    /// 키프레임 간격
    pub keyframe_interval: Option<usize>,
}

/// VP8/VP9 디코더 설정
#[derive(Clone, Copy, Debug)]
pub struct VpxDecoderConfig {
    pub codec: VpxVideoCodecId,
}

/// 인코딩된 프레임 반복자
pub struct EncodeFrames<'a> {
    ctx: &'a mut vpx_codec_ctx_t,
    iter: vpx_codec_iter_t,
}

impl<'a> Iterator for EncodeFrames<'a> {
    type Item = EncodeFrame<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            unsafe {
                let pkt = vpx_codec_get_cx_data(self.ctx, &mut self.iter);
                if pkt.is_null() {
                    return None;
                } else if (*pkt).kind == vpx_codec_cx_pkt_kind::VPX_CODEC_CX_FRAME_PKT {
                    let f = &(*pkt).data.frame;
                    return Some(Self::Item {
                        data: slice::from_raw_parts(f.buf as _, f.sz as _),
                        key: (f.flags & VPX_FRAME_IS_KEY) != 0,
                        pts: f.pts,
                    });
                } else {
                    // 패킷 무시
                }
            }
        }
    }
}

impl VpxDecoder {
    /// 새로운 디코더 생성
    ///
    /// # 에러
    /// 기본 libvpx에서 VP9 디코더를 제공하지 않으면 실패할 수 있다
    pub fn new(config: VpxDecoderConfig) -> Result<Self> {
        // vpx_codec_ctx는 초기화되지 않은 상태에서 UB를 유발할 수 있는
        // 필드가 없는 repr(C) 구조체이므로 이것은 안전하다
        let i = match config.codec {
            VpxVideoCodecId::VP8 => call_vpx_ptr!(vpx_codec_vp8_dx()),
            VpxVideoCodecId::VP9 => call_vpx_ptr!(vpx_codec_vp9_dx()),
        };
        let mut ctx = Default::default();
        let cfg = vpx_codec_dec_cfg_t {
            threads: codec_thread_num(64) as _,
            w: 0,
            h: 0,
        };
        /*
        unsafe {
            println!("{}", vpx_codec_get_caps(i));
        }
        */
        call_vpx!(vpx_codec_dec_init_ver(
            &mut ctx,
            i,
            &cfg,
            0,
            VPX_DECODER_ABI_VERSION as _,
        ));
        Ok(Self { ctx })
    }

    /// 압축된 데이터를 인코더에 입력
    ///
    /// `data` 슬라이스는 디코더로 전송된다.
    ///
    /// `vpx_codec_decode` 호출과 일치한다.
    pub fn decode<'a>(&'a mut self, data: &[u8]) -> Result<DecodeFrames<'a>> {
        call_vpx!(vpx_codec_decode(
            &mut self.ctx,
            data.as_ptr(),
            data.len() as _,
            ptr::null_mut(),
            0,
        ));

        Ok(DecodeFrames {
            ctx: &mut self.ctx,
            iter: ptr::null(),
        })
    }

    /// 대기 중인 프레임을 디코더에서 반환하도록 요청
    pub fn flush<'a>(&'a mut self) -> Result<DecodeFrames<'a>> {
        call_vpx!(vpx_codec_decode(
            &mut self.ctx,
            ptr::null(),
            0,
            ptr::null_mut(),
            0
        ));
        Ok(DecodeFrames {
            ctx: &mut self.ctx,
            iter: ptr::null(),
        })
    }
}

impl Drop for VpxDecoder {
    fn drop(&mut self) {
        unsafe {
            let result = vpx_codec_destroy(&mut self.ctx);
            if result != VPX_CODEC_OK {
                panic!("failed to destroy vpx codec");
            }
        }
    }
}

/// 디코딩된 프레임 반복자
pub struct DecodeFrames<'a> {
    ctx: &'a mut vpx_codec_ctx_t,
    iter: vpx_codec_iter_t,
}

impl<'a> Iterator for DecodeFrames<'a> {
    type Item = Image;
    fn next(&mut self) -> Option<Self::Item> {
        let img = unsafe { vpx_codec_get_frame(self.ctx, &mut self.iter) };
        if img.is_null() {
            return None;
        } else {
            return Some(Image(img));
        }
    }
}

// 참고: https://chromium.googlesource.com/webm/libvpx/+/bali/vpx/src/vpx_image.c
/// VPX 디코딩된 이미지 래퍼
pub struct Image(*mut vpx_image_t);

impl Image {
    /// 새로운 빈 이미지 생성
    #[inline]
    pub fn new() -> Self {
        Self(std::ptr::null_mut())
    }

    /// 이미지가 null인지 확인
    #[inline]
    pub fn is_null(&self) -> bool {
        self.0.is_null()
    }

    /// 이미지의 픽셀 형식 반환
    #[inline]
    pub fn format(&self) -> vpx_img_fmt_t {
        // VPX_IMG_FMT_I420
        self.inner().fmt
    }

    /// 내부 vpx_image_t 참조 반환
    #[inline]
    pub fn inner(&self) -> &vpx_image_t {
        unsafe { &*self.0 }
    }
}

impl GoogleImage for Image {
    #[inline]
    fn width(&self) -> usize {
        self.inner().d_w as _
    }

    #[inline]
    fn height(&self) -> usize {
        self.inner().d_h as _
    }

    #[inline]
    fn stride(&self) -> Vec<i32> {
        self.inner().stride.iter().map(|x| *x as i32).collect()
    }

    #[inline]
    fn planes(&self) -> Vec<*mut u8> {
        self.inner().planes.iter().map(|p| *p as *mut u8).collect()
    }

    fn chroma(&self) -> Chroma {
        match self.inner().fmt {
            vpx_img_fmt::VPX_IMG_FMT_I444 => Chroma::I444,
            _ => Chroma::I420,
        }
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { vpx_img_free(self.0) };
        }
    }
}

// VPX 코덱 컨텍스트는 다중 스레드에서 안전하게 전송될 수 있음
unsafe impl Send for vpx_codec_ctx_t {}
