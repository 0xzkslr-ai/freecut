//! A reusable top-down BGRA surface owned entirely by the recording worker.
use crate::platform::Monitor;
use std::{
    mem::{size_of, zeroed},
    ptr::null_mut,
};
use windows_sys::Win32::{
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::{
        DrawIconEx, GetCursorInfo, GetIconInfo, CURSORINFO, CURSOR_SHOWING, DI_NORMAL, ICONINFO,
    },
};

pub(super) struct ScreenCapture {
    screen: HDC,
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    pixels: *mut std::ffi::c_void,
    region: Monitor,
}

impl ScreenCapture {
    pub(super) fn new(region: Monitor) -> Result<Self, String> {
        unsafe {
            let mut capture = Self {
                screen: GetDC(null_mut()),
                dc: null_mut(),
                bitmap: null_mut(),
                previous: null_mut(),
                pixels: null_mut(),
                region,
            };
            if capture.screen.is_null() {
                return Err("无法访问录屏桌面。".into());
            }
            capture.dc = CreateCompatibleDC(capture.screen);
            if capture.dc.is_null() {
                return Err("无法创建录屏绘图上下文。".into());
            }
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = capture.region.width as i32;
            info.bmiHeader.biHeight = -(capture.region.height as i32);
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            capture.bitmap = CreateDIBSection(
                capture.screen,
                &info,
                DIB_RGB_COLORS,
                &mut capture.pixels,
                null_mut(),
                0,
            );
            if capture.bitmap.is_null() || capture.pixels.is_null() {
                return Err("无法创建录屏画面缓冲区。".into());
            }
            capture.previous = SelectObject(capture.dc, capture.bitmap);
            if capture.previous.is_null() || capture.previous as isize == -1 {
                return Err("无法绑定录屏画面缓冲区。".into());
            }
            Ok(capture)
        }
    }

    pub(super) fn frame(&mut self) -> Result<&[u8], String> {
        unsafe {
            if BitBlt(
                self.dc,
                0,
                0,
                self.region.width as i32,
                self.region.height as i32,
                self.screen,
                self.region.x,
                self.region.y,
                SRCCOPY | CAPTUREBLT,
            ) == 0
            {
                return Err("录屏采集失败，请检查显示器和桌面会话。".into());
            }
            self.draw_cursor();
            if GdiFlush() == 0 {
                return Err("无法同步录屏画面。".into());
            }
            Ok(std::slice::from_raw_parts(
                self.pixels.cast(),
                self.region.width as usize * self.region.height as usize * 4,
            ))
        }
    }

    unsafe fn draw_cursor(&self) {
        unsafe {
            let mut cursor: CURSORINFO = zeroed();
            cursor.cbSize = size_of::<CURSORINFO>() as u32;
            if GetCursorInfo(&mut cursor) == 0 || cursor.flags & CURSOR_SHOWING == 0 {
                return;
            }
            let mut icon: ICONINFO = zeroed();
            if GetIconInfo(cursor.hCursor, &mut icon) == 0 {
                return;
            }
            DrawIconEx(
                self.dc,
                cursor.ptScreenPos.x - self.region.x - icon.xHotspot as i32,
                cursor.ptScreenPos.y - self.region.y - icon.yHotspot as i32,
                cursor.hCursor,
                0,
                0,
                0,
                null_mut(),
                DI_NORMAL,
            );
            if !icon.hbmMask.is_null() {
                DeleteObject(icon.hbmMask);
            }
            if !icon.hbmColor.is_null() {
                DeleteObject(icon.hbmColor);
            }
        }
    }
}

impl Drop for ScreenCapture {
    fn drop(&mut self) {
        unsafe {
            if !self.dc.is_null() && !self.previous.is_null() && self.previous as isize != -1 {
                SelectObject(self.dc, self.previous);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
            if !self.screen.is_null() {
                ReleaseDC(null_mut(), self.screen);
            }
        }
    }
}
