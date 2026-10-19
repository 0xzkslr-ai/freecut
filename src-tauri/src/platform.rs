//! Small Windows adapter: monitor coordinates and pixels are always physical pixels.
use crate::imaging::Image;
use serde::Serialize;
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{GlobalFree, HWND, LPARAM, POINT, RECT},
    Graphics::{Dwm::DwmFlush, Gdi::*},
    System::{DataExchange::*, Memory::*},
    UI::WindowsAndMessaging::{GetCursorPos, ShowWindow, SW_HIDE},
};

#[derive(Clone, Debug, Serialize)]
pub struct Monitor {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

unsafe extern "system" fn collect_monitor(
    handle: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> i32 {
    let mut info: MONITORINFOEXW = zeroed();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(handle, &mut info.monitorInfo) != 0 {
        let rect = info.monitorInfo.rcMonitor;
        let len = info
            .szDevice
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.szDevice.len());
        let id = String::from_utf16_lossy(&info.szDevice[..len]);
        let monitors = &mut *(data as *mut Vec<Monitor>);
        monitors.push(Monitor {
            name: id.trim_start_matches("\\\\.\\").to_owned(),
            id,
            x: rect.left,
            y: rect.top,
            width: (rect.right - rect.left) as u32,
            height: (rect.bottom - rect.top) as u32,
            primary: info.monitorInfo.dwFlags & 1 != 0,
        });
    }
    1
}

pub fn monitors() -> Result<Vec<Monitor>, String> {
    let mut result: Vec<Monitor> = Vec::new();
    // SAFETY: EnumDisplayMonitors invokes the callback synchronously with our live Vec.
    if unsafe {
        EnumDisplayMonitors(
            null_mut(),
            null(),
            Some(collect_monitor),
            &mut result as *mut _ as LPARAM,
        )
    } == 0
        || result.is_empty()
    {
        return Err("没有找到可截图的显示器。".into());
    }
    result.sort_by_key(|m| (!m.primary, m.x, m.y));
    Ok(result)
}

pub fn cursor_monitor(monitors: &[Monitor]) -> Result<Monitor, String> {
    let mut point = POINT { x: 0, y: 0 };
    let positioned = unsafe { GetCursorPos(&mut point) } != 0;
    monitors
        .iter()
        .find(|m| {
            positioned
                && point.x >= m.x
                && point.y >= m.y
                && (point.x as i64) < m.x as i64 + m.width as i64
                && (point.y as i64) < m.y as i64 + m.height as i64
        })
        .or_else(|| monitors.iter().find(|m| m.primary))
        .or(monitors.first())
        .cloned()
        .ok_or("没有找到可截图的显示器。".into())
}

/// Hide a live application window before reading the desktop pixels.
///
/// # Safety
/// `hwnd` must identify a live window owned by the calling application.
pub unsafe fn hide_for_capture(hwnd: HWND) {
    // Synchronous hide + compositor flush, with no timer or artificial capture delay.
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
        DwmFlush();
    }
}

pub fn capture(monitor: &Monitor) -> Result<Image, String> {
    let (w, h) = (monitor.width, monitor.height);
    if w == 0 || h == 0 || w > 32768 || h > 32768 || w as u64 * h as u64 > 100_000_000 {
        return Err("显示器尺寸无效或过大。".into());
    }
    // All GDI objects are released even if one of the later calls fails.
    unsafe {
        let screen = GetDC(null_mut());
        if screen.is_null() {
            return Err("无法访问屏幕，请检查桌面会话。".into());
        }
        let dc = CreateCompatibleDC(screen);
        if dc.is_null() {
            ReleaseDC(null_mut(), screen);
            return Err("无法创建截图上下文。".into());
        }
        let bitmap = CreateCompatibleBitmap(screen, w as i32, h as i32);
        if bitmap.is_null() {
            DeleteDC(dc);
            ReleaseDC(null_mut(), screen);
            return Err("无法创建屏幕图像。".into());
        }
        let previous = SelectObject(dc, bitmap);
        let copied = BitBlt(
            dc,
            0,
            0,
            w as i32,
            h as i32,
            screen,
            monitor.x,
            monitor.y,
            SRCCOPY | CAPTUREBLT,
        );
        SelectObject(dc, previous);
        let mut info: BITMAPINFO = zeroed();
        info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = w as i32;
        info.bmiHeader.biHeight = -(h as i32);
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut rgba = vec![0; w as usize * h as usize * 4];
        let rows = if copied != 0 {
            GetDIBits(
                screen,
                bitmap,
                0,
                h,
                rgba.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(null_mut(), screen);
        if rows != h as i32 {
            return Err("截图失败，显示器可能已断开，请重试。".into());
        }
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.swap(0, 2);
            pixel[3] = 255;
        }
        Ok(Image {
            width: w,
            height: h,
            rgba,
        })
    }
}

/// A top-down, straight-alpha DIBV5, alongside an exact PNG for alpha-aware clients.
pub fn dib_bytes(image: &Image) -> Result<Vec<u8>, String> {
    image.validate()?;
    let mut header: BITMAPV5HEADER = unsafe { zeroed() };
    header.bV5Size = size_of::<BITMAPV5HEADER>() as u32;
    header.bV5Width = image.width as i32;
    header.bV5Height = -(image.height as i32);
    header.bV5Planes = 1;
    header.bV5BitCount = 32;
    header.bV5Compression = BI_BITFIELDS;
    header.bV5SizeImage = image.rgba.len() as u32;
    header.bV5RedMask = 0x00ff0000;
    header.bV5GreenMask = 0x0000ff00;
    header.bV5BlueMask = 0x000000ff;
    header.bV5AlphaMask = 0xff000000;
    header.bV5CSType = 0x73524742;
    header.bV5Intent = LCS_GM_IMAGES as u32;
    let mut bytes = vec![0; size_of::<BITMAPV5HEADER>() + image.rgba.len()];
    unsafe {
        std::ptr::copy_nonoverlapping(
            &header as *const _ as *const u8,
            bytes.as_mut_ptr(),
            size_of::<BITMAPV5HEADER>(),
        );
    }
    for (src, dst) in image
        .rgba
        .chunks_exact(4)
        .zip(bytes[size_of::<BITMAPV5HEADER>()..].chunks_exact_mut(4))
    {
        dst.copy_from_slice(&[src[2], src[1], src[0], src[3]]);
    }
    Ok(bytes)
}

struct GlobalBlock(windows_sys::Win32::Foundation::HGLOBAL);
impl GlobalBlock {
    fn new(bytes: &[u8]) -> Result<Self, String> {
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
            if handle.is_null() {
                return Err("剪贴板内存不足。".into());
            }
            let block = Self(handle);
            let ptr = GlobalLock(handle);
            if ptr.is_null() {
                return Err("无法访问剪贴板内存。".into());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast(), bytes.len());
            GlobalUnlock(handle);
            Ok(block)
        }
    }
    unsafe fn transfer(mut self, format: u32) -> Result<(), String> {
        if SetClipboardData(format, self.0).is_null() {
            return Err("无法写入剪贴板，请重试。".into());
        }
        self.0 = null_mut();
        Ok(())
    }
}
impl Drop for GlobalBlock {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                GlobalFree(self.0);
            }
        }
    }
}

/// Publish PNG and native DIBV5 formats to the Windows clipboard.
///
/// # Safety
/// `owner` must identify a live window owned by the calling application.
pub unsafe fn copy_image(owner: HWND, image: &Image, png: &[u8]) -> Result<(), String> {
    let dib = GlobalBlock::new(&dib_bytes(image)?)?;
    let png = GlobalBlock::new(png)?;
    let name: Vec<u16> = "PNG\0".encode_utf16().collect();
    unsafe {
        let format = RegisterClipboardFormatW(name.as_ptr());
        if format == 0 {
            return Err("无法注册 PNG 剪贴板格式。".into());
        }
        if OpenClipboard(owner) == 0 {
            return Err("剪贴板正被其他应用占用，请稍后重试。".into());
        }
        let result = (|| {
            if EmptyClipboard() == 0 {
                return Err("无法清空剪贴板，请重试。".into());
            }
            png.transfer(format)?;
            dib.transfer(17) // CF_DIBV5
        })();
        CloseClipboard();
        result
    }
}
