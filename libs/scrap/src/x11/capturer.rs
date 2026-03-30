use super::ffi::*;
use super::Display;
use hbb_common::libc;
use std::{io, ptr, slice};

/// X11 디스플레이에서 MIT-SHM을 사용하여 화면을 캡처합니다.
pub struct Capturer {
    // X11 디스플레이
    display: Display,
    // 공유 메모리 ID
    shmid: i32,
    // XCB 공유 메모리 ID
    xcbid: u32,
    // 공유 메모리 버퍼 포인터
    buffer: *const u8,

    // 버퍼 크기
    size: usize,
    // 프레임 비교용 저장된 데이터 (더 빠른 비교 및 복사를 위해)
    saved_raw_data: Vec<u8>,
}

impl Capturer {
    /// X11 디스플레이에서 화면 캡처기를 생성합니다.
    pub fn new(display: Display) -> io::Result<Capturer> {
        // === 화면 크기 계산 ===
        let pixel_width = display.pixfmt().bytes_per_pixel();
        let rect = display.rect();
        let size = (rect.w as usize) * (rect.h as usize) * pixel_width;

        // === 공유 메모리 세그먼트 생성 ===
        let shmid = unsafe {
            libc::shmget(
                libc::IPC_PRIVATE,
                size,
                // 모든 사용자가 읽고 쓸 수 있음
                libc::IPC_CREAT | 0o777,
            )
        };

        if shmid == -1 {
            return Err(io::Error::last_os_error());
        }

        // === 세그먼트를 읽기 가능한 주소에 연결 ===
        let buffer = unsafe { libc::shmat(shmid, ptr::null(), libc::SHM_RDONLY) } as *mut u8;

        if buffer as isize == -1 {
            return Err(io::Error::last_os_error());
        }

        // Attach the segment to XCB.

        let server = display.server().raw();
        let xcbid = unsafe { xcb_generate_id(server) };
        unsafe {
            xcb_shm_attach(
                server,
                xcbid,
                shmid as u32,
                0, // False, i.e. not read-only.
            );
        }

        let c = Capturer {
            display,
            shmid,
            xcbid,
            buffer,
            size,
            saved_raw_data: Vec::new(),
        };
        Ok(c)
    }

    pub fn display(&self) -> &Display {
        &self.display
    }

    fn get_image(&self) {
        let rect = self.display.rect();
        unsafe {
            let request = xcb_shm_get_image_unchecked(
                self.display.server().raw(),
                self.display.root(),
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                !0,
                XCB_IMAGE_FORMAT_Z_PIXMAP,
                self.xcbid,
                0,
            );
            let response =
                xcb_shm_get_image_reply(self.display.server().raw(), request, ptr::null_mut());
            libc::free(response as *mut _);
        }
    }

    pub fn frame<'b>(&'b mut self) -> std::io::Result<&'b [u8]> {
        self.get_image();
        let result = unsafe { slice::from_raw_parts(self.buffer, self.size) };
        crate::would_block_if_equal(&mut self.saved_raw_data, result)?;
        Ok(result)
    }
}

impl Drop for Capturer {
    fn drop(&mut self) {
        unsafe {
            // Detach segment from XCB.
            xcb_shm_detach(self.display.server().raw(), self.xcbid);
            // Detach segment from our space.
            libc::shmdt(self.buffer as *mut _);
            // Destroy the shared memory segment.
            libc::shmctl(self.shmid, libc::IPC_RMID, ptr::null_mut());
        }
    }
}
