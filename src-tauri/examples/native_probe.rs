//! Opt-in Windows integration-test helper. Never bundled with FreeCut.
#[cfg(windows)]
mod native {
    use std::{
        io::{self, Write},
        mem::zeroed,
        ptr::{null, null_mut},
    };
    use tauri_plugin_global_shortcut::{Modifiers, Shortcut};
    use windows_sys::Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{DataExchange::*, LibraryLoader::GetModuleHandleW, Memory::*},
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    };
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }
    static MAP_X: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
    static MAP_Y: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
    unsafe extern "system" fn map_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        use std::sync::atomic::Ordering;
        if msg == WM_APP {
            MAP_X.store(wparam as i32, Ordering::Relaxed);
            MAP_Y.store(lparam as i32, Ordering::Relaxed);
            InvalidateRect(hwnd, null(), 0);
            UpdateWindow(hwnd);
            return 0;
        }
        if msg == WM_PAINT {
            let mut paint: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(hwnd, &mut paint);
            let mut rect: RECT = zeroed();
            GetClientRect(hwnd, &mut rect);
            let (w, h) = (rect.right, rect.bottom);
            let (offset_x, offset_y) =
                (MAP_X.load(Ordering::Relaxed), MAP_Y.load(Ordering::Relaxed));
            let mut pixels = Vec::with_capacity(w as usize * h as usize * 4);
            for y in offset_y..offset_y + h {
                for x in offset_x..offset_x + w {
                    let mut n = (x.div_euclid(11) as u32).wrapping_mul(0x45d9f3b)
                        ^ (y.div_euclid(13) as u32).wrapping_mul(0x119de1f3);
                    n = (n ^ (n >> 16)).wrapping_mul(0x45d9f3b);
                    let detail = (x * 17 + y * 29).rem_euclid(29) as u8;
                    pixels.extend_from_slice(&[
                        ((n >> 16) as u8 / 2) + detail,
                        ((n >> 8) as u8 / 2) + detail,
                        (n as u8 / 2) + detail,
                        255,
                    ]);
                }
            }
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = w;
            info.bmiHeader.biHeight = -h;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            SetDIBitsToDevice(
                dc,
                0,
                0,
                w as u32,
                h as u32,
                0,
                0,
                0,
                h as u32,
                pixels.as_ptr().cast(),
                &info,
                DIB_RGB_COLORS,
            );
            EndPaint(hwnd, &paint);
            return 0;
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
    struct Search {
        pid: u32,
        class: Option<String>,
        title: Option<String>,
        found: HWND,
    }
    unsafe extern "system" fn find(hwnd: HWND, data: LPARAM) -> i32 {
        let search = &mut *(data as *mut Search);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if search.pid != 0 && pid != search.pid {
            return 1;
        }
        let mut class = [0; 128];
        let n = GetClassNameW(hwnd, class.as_mut_ptr(), 128);
        let mut title = [0; 256];
        let len = GetWindowTextW(hwnd, title.as_mut_ptr(), 256);
        if search
            .class
            .as_ref()
            .is_none_or(|s| s == &String::from_utf16_lossy(&class[..n as usize]))
            && search
                .title
                .as_ref()
                .is_none_or(|s| s == &String::from_utf16_lossy(&title[..len as usize]))
        {
            search.found = hwnd;
            return 0;
        }
        1
    }
    unsafe fn find_window(pid: u32, class: Option<&str>, title: Option<&str>) -> HWND {
        let mut search = Search {
            pid,
            class: class.map(str::to_owned),
            title: title.map(str::to_owned),
            found: null_mut(),
        };
        EnumWindows(Some(find), &mut search as *mut _ as LPARAM);
        search.found
    }
    unsafe extern "system" fn find_edit(hwnd: HWND, data: LPARAM) -> i32 {
        let result = &mut *(data as *mut HWND);
        let mut class = [0; 128];
        let n = GetClassNameW(hwnd, class.as_mut_ptr(), 128);
        if String::from_utf16_lossy(&class[..n as usize]) == "Edit"
            && IsWindowVisible(hwnd) != 0
            && IsWindowEnabled(hwnd) != 0
        {
            let mut rect: RECT = zeroed();
            GetWindowRect(hwnd, &mut rect);
            let mut previous: RECT = zeroed();
            if !result.is_null() && !(*result).is_null() {
                GetWindowRect(*result, &mut previous);
            }
            if (*result).is_null() || rect.top > previous.top {
                *result = hwnd;
            }
        }
        1
    }
    fn binding(text: &str) -> (Shortcut, u32, u32) {
        let shortcut: Shortcut = text.parse().expect("shortcut");
        let mut mods = 0;
        if shortcut.mods.contains(Modifiers::CONTROL) {
            mods |= MOD_CONTROL;
        }
        if shortcut.mods.contains(Modifiers::ALT) {
            mods |= MOD_ALT;
        }
        if shortcut.mods.contains(Modifiers::SHIFT) {
            mods |= MOD_SHIFT;
        }
        let key = shortcut.key.to_string();
        let vk = if let Some(key) = key.strip_prefix("Key") {
            key.as_bytes()[0] as u32
        } else if let Some(key) = key.strip_prefix("Digit") {
            key.as_bytes()[0] as u32
        } else {
            0x70 + key.strip_prefix('F').unwrap().parse::<u32>().unwrap() - 1
        };
        (shortcut, mods, vk)
    }
    pub fn run() {
        let args: Vec<String> = std::env::args().collect();
        unsafe {
            match args[1].as_str() {
                "running" => {
                    let hwnd = find_window(0, None, Some("FreeCut · 自由形状截图"));
                    let mut pid = 0;
                    if !hwnd.is_null() {
                        GetWindowThreadProcessId(hwnd, &mut pid);
                    }
                    println!("{pid}");
                }
                "fixture" | "map-fixture" => {
                    SetProcessDpiAwarenessContext(
                        windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                    );
                    let name = wide("FreeCutNativeTestPattern");
                    let instance = GetModuleHandleW(null());
                    let brush = CreateSolidBrush(0x00563412);
                    let class = WNDCLASSW {
                        lpfnWndProc: Some(if args[1] == "map-fixture" {
                            map_proc
                        } else {
                            DefWindowProcW
                        }),
                        hInstance: instance,
                        lpszClassName: name.as_ptr(),
                        hbrBackground: brush,
                        ..zeroed()
                    };
                    RegisterClassW(&class);
                    let hwnd = CreateWindowExW(
                        if args[1] == "map-fixture" {
                            WS_EX_TOPMOST
                        } else {
                            0
                        },
                        name.as_ptr(),
                        wide("FreeCut native test pattern").as_ptr(),
                        WS_POPUP | WS_VISIBLE,
                        0,
                        0,
                        GetSystemMetrics(SM_CXSCREEN),
                        GetSystemMetrics(SM_CYSCREEN),
                        null_mut(),
                        null_mut(),
                        instance,
                        null(),
                    );
                    assert!(!hwnd.is_null());
                    ShowWindow(hwnd, SW_SHOW);
                    UpdateWindow(hwnd);
                    println!("ready");
                    io::stdout().flush().unwrap();
                    let mut message: MSG = zeroed();
                    while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                    DeleteObject(brush);
                }
                "post" => {
                    let pid = args[2].parse().unwrap();
                    let (shortcut, mods, vk) = binding(&args[3]);
                    let hwnd = find_window(pid, Some("global_hotkey_app"), None);
                    assert!(!hwnd.is_null(), "hotkey window missing");
                    assert_ne!(
                        PostMessageW(
                            hwnd,
                            WM_HOTKEY,
                            shortcut.id() as usize,
                            ((vk << 16) | mods) as isize
                        ),
                        0
                    );
                    println!("posted");
                }
                "map-move" => {
                    let hwnd = find_window(
                        args[2].parse().unwrap(),
                        Some("FreeCutNativeTestPattern"),
                        None,
                    );
                    assert!(!hwnd.is_null(), "map fixture missing");
                    SendMessageW(
                        hwnd,
                        WM_APP,
                        args[3].parse::<i32>().unwrap() as usize,
                        args[4].parse::<i32>().unwrap() as isize,
                    );
                    println!("moved");
                }
                "check" | "reserve" => {
                    let (_, mods, vk) = binding(&args[2]);
                    let registered =
                        RegisterHotKey(null_mut(), 0x7531, mods | MOD_NOREPEAT, vk) != 0;
                    println!("{registered}");
                    io::stdout().flush().unwrap();
                    if registered {
                        if args[1] == "reserve" {
                            let mut line = String::new();
                            io::stdin().read_line(&mut line).unwrap();
                        }
                        UnregisterHotKey(null_mut(), 0x7531);
                    }
                }
                "visible" => {
                    let title = if args[3] == "overlay" {
                        "FreeCut · 选择截图区域"
                    } else if args[3] == "scroll" {
                        "FreeCut · 滚动截图"
                    } else {
                        "FreeCut · 自由形状截图"
                    };
                    let hwnd = find_window(args[2].parse().unwrap(), None, Some(title));
                    println!("{}", !hwnd.is_null() && IsWindowVisible(hwnd) != 0);
                }
                "save-dialog" => {
                    let hwnd = find_window(args[2].parse().unwrap(), Some("#32770"), None);
                    assert!(!hwnd.is_null(), "save dialog missing");
                    let mut edit: HWND = null_mut();
                    EnumChildWindows(hwnd, Some(find_edit), &mut edit as *mut HWND as LPARAM);
                    assert!(!edit.is_null(), "filename edit missing");
                    SetWindowTextW(edit, wide(&args[3]).as_ptr());
                    PostMessageW(hwnd, WM_COMMAND, IDOK as usize, 0);
                    println!("saved");
                }
                "clipboard" => {
                    assert_ne!(OpenClipboard(null_mut()), 0);
                    let format = RegisterClipboardFormatW(wide("PNG").as_ptr());
                    let memory = GetClipboardData(format);
                    assert!(!memory.is_null());
                    let length = GlobalSize(memory);
                    let data = GlobalLock(memory);
                    assert!(!data.is_null());
                    let bytes = std::slice::from_raw_parts(data.cast::<u8>(), length);
                    std::fs::write(&args[2], bytes).unwrap();
                    GlobalUnlock(memory);
                    let dib = IsClipboardFormatAvailable(17) != 0;
                    CloseClipboard();
                    assert!(dib, "DIBV5 missing");
                    println!("{length}");
                }
                _ => panic!("unknown command"),
            }
        }
    }
    use windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext;
}
fn main() {
    #[cfg(windows)]
    native::run();
}
