#![windows_subsystem = "windows"]

mod audio;
mod icons;
mod keys;
mod updates;

use keys::{Keyboard, Shortcut, modifier};
use std::{cell::RefCell, path::PathBuf};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{Com::*, LibraryLoader::GetModuleHandleW, Threading::CreateMutexW},
        UI::{Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, Result, w},
};

const TRAY: u32 = WM_APP + 1;
const TOGGLE: u32 = WM_APP + 2;
const CAPTURED: u32 = WM_APP + 3;
const SHOW_SETTINGS: u32 = WM_APP + 4;
const CHOOSE: usize = 101;
const SAVE: usize = 102;
const CANCEL: usize = 103;
const SETTING: usize = 201;
const QUIT: usize = 202;
const CHECK_UPDATES: usize = 203;
const TRAY_RETRY_TIMER: usize = 301;
const TRAY_FALLBACK_ATTEMPTS: u32 = 10;

struct App {
    hwnd: HWND,
    field: HWND,
    status: HWND,
    path: PathBuf,
    shortcut: Shortcut,
    pending: Shortcut,
    keys: Keyboard,
    visible: bool,
    recording: bool,
    candidate: Option<Shortcut>,
    tray_available: bool,
    tray_retry_count: u32,
    forced_tray_failure: bool,
    icons: Option<icons::Icons>,
    mic_state: icons::State,
    testing: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            hwnd: HWND::default(),
            field: HWND::default(),
            status: HWND::default(),
            path: PathBuf::new(),
            shortcut: Shortcut::default(),
            pending: Shortcut::default(),
            keys: Keyboard::default(),
            visible: false,
            recording: false,
            candidate: None,
            tray_available: false,
            tray_retry_count: 0,
            forced_tray_failure: false,
            icons: None,
            mic_state: icons::State::Unknown,
            testing: false,
        }
    }
}
thread_local! { static APP: RefCell<App> = RefCell::new(App::default()); }
thread_local! { static MONITOR: RefCell<Option<audio::Monitor>> = const { RefCell::new(None) }; }
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn config_path() -> std::result::Result<PathBuf, String> {
    let base = std::env::var_os("APPDATA").ok_or("Windows APPDATA is unavailable")?;
    Ok(PathBuf::from(base).join("MicKey").join("shortcut.txt"))
}

fn load(path: &std::path::Path) -> std::result::Result<Option<Shortcut>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read settings: {e}")),
    };
    let values: Vec<_> = text.split_whitespace().collect();
    if values.len() == 2 {
        if let (Ok(key), Ok(mods)) = (values[0].parse(), values[1].parse()) {
            let s = Shortcut { key, mods };
            if s.valid() {
                return Ok(Some(s));
            }
        }
    }
    Err("Settings are invalid. Please choose a shortcut and save again.".into())
}

fn save(path: &std::path::Path, shortcut: Shortcut) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    // Write a temporary file, then replace atomically using the Windows API.
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, format!("{} {}\n", shortcut.key, shortcut.mods))?;
    unsafe {
        use windows::Win32::Storage::FileSystem::*;
        MoveFileExW(
            PCWSTR(wide(&temp.to_string_lossy()).as_ptr()),
            PCWSTR(wide(&path.to_string_lossy()).as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(std::io::Error::other)
    }
}

fn key_name(key: u16) -> String {
    match key {
        0x11 => "Ctrl".into(),
        0xa2 => "Left Ctrl".into(),
        0xa3 => "Right Ctrl".into(),
        0x10 => "Shift".into(),
        0xa0 => "Left Shift".into(),
        0xa1 => "Right Shift".into(),
        0x12 => "Alt".into(),
        0xa4 => "Left Alt".into(),
        0xa5 => "Right Alt".into(),
        0x5b => "Left Win".into(),
        0x5c => "Right Win".into(),
        0x30..=0x39 | 0x41..=0x5a => char::from_u32(key as u32).unwrap().to_string(),
        0x70..=0x87 => format!("F{}", key - 0x6f),
        _ => unsafe {
            let scan = MapVirtualKeyW(key as u32, MAPVK_VK_TO_VSC_EX);
            let bits = ((scan & 0xff) << 16) | if scan & 0xff00 != 0 { 1 << 24 } else { 0 };
            let mut buf = [0u16; 80];
            let len = GetKeyNameTextW(bits as i32, &mut buf);
            if len > 0 {
                String::from_utf16_lossy(&buf[..len as usize])
            } else {
                format!("Key 0x{key:02X}")
            }
        },
    }
}

fn shortcut_name(s: Shortcut) -> String {
    let mut parts = Vec::new();
    for (bit, name) in [
        (keys::CTRL, "Ctrl"),
        (keys::SHIFT, "Shift"),
        (keys::ALT, "Alt"),
        (keys::WIN, "Win"),
    ] {
        if s.mods & bit != 0 {
            parts.push(name.to_string());
        }
    }
    parts.push(key_name(s.key));
    parts.join(" + ")
}

fn message(text: &str, error: bool) {
    let hwnd = APP.with(|a| a.borrow().hwnd);
    unsafe {
        MessageBoxW(
            Some(hwnd),
            PCWSTR(wide(text).as_ptr()),
            w!("MicKey"),
            MB_OK
                | if error {
                    MB_ICONERROR
                } else {
                    MB_ICONINFORMATION
                },
        );
    }
}

fn set_text(hwnd: HWND, text: &str) {
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(wide(text).as_ptr()));
    }
}

fn update_field() {
    let (field, pending) = APP.with(|a| {
        let a = a.borrow();
        (a.field, a.pending)
    });
    set_text(field, &shortcut_name(pending));
}

fn update_status() {
    let mute = audio::muted();
    let state = icons::State::from_mute(mute.as_ref().ok().copied());
    let (hwnd, root) = APP.with(|a| {
        let mut a = a.borrow_mut();
        a.mic_state = state;
        (a.status, a.hwnd)
    });
    let text = match mute {
        Ok(true) => "สถานะไมค์เริ่มต้น: ปิดเสียง (Muted)".to_string(),
        Ok(false) => "สถานะไมค์เริ่มต้น: เปิดเสียง (On)".to_string(),
        Err(e) => format!("อ่านสถานะไมค์ไม่ได้: {e}"),
    };
    set_text(hwnd, &text);
    let icon = APP.with(|a| a.borrow().icons.as_ref().map(|icons| icons.get(state)));
    if let Some(icon) = icon {
        unsafe {
            SendMessageW(
                root,
                WM_SETICON,
                Some(WPARAM(ICON_SMALL as usize)),
                Some(LPARAM(icon.0 as isize)),
            );
            SendMessageW(
                root,
                WM_SETICON,
                Some(WPARAM(ICON_BIG as usize)),
                Some(LPARAM(icon.0 as isize)),
            );
        }
    }
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &tray_data());
    }
}

fn tray_data() -> NOTIFYICONDATAW {
    let (hwnd, shortcut, state, icon) = APP.with(|a| {
        let a = a.borrow();
        (
            a.hwnd,
            a.shortcut,
            a.mic_state,
            a.icons
                .as_ref()
                .map(|icons| icons.get(a.mic_state))
                .unwrap_or_default(),
        )
    });
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: TRAY,
        ..Default::default()
    };
    data.hIcon = icon;
    let tip = wide(&format!(
        "MicKey — {} | {} | คลิกขวาเพื่อตั้งค่า",
        state.label(),
        shortcut_name(shortcut)
    ));
    let len = tip.len().min(data.szTip.len() - 1);
    data.szTip[..len].copy_from_slice(&tip[..len]);
    data
}

fn show_settings() {
    let hwnd = APP.with(|a| {
        let mut a = a.borrow_mut();
        a.visible = true;
        a.recording = false;
        a.candidate = None;
        a.pending = a.shortcut;
        a.keys.cancel();
        a.hwnd
    });
    update_field();
    update_status();
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
}

fn hide_settings() {
    // Explorer may have restarted, or tray registration may have failed earlier.
    ensure_tray();
    let hwnd = APP.with(|a| {
        let mut a = a.borrow_mut();
        a.visible = false;
        a.recording = false;
        a.candidate = None;
        a.keys.cancel();
        a.hwnd
    });
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    if !APP.with(|a| a.borrow().tray_available) {
        retry_tray(hwnd);
    }
}

fn retry_tray(hwnd: HWND) {
    APP.with(|a| a.borrow_mut().tray_retry_count = 0);
    unsafe {
        if SetTimer(Some(hwnd), TRAY_RETRY_TIMER, 1000, None) == 0 {
            // A minimized taskbar window remains accessible if no timer is available.
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
        }
    }
}

fn ensure_tray() -> bool {
    if APP.with(|a| a.borrow().forced_tray_failure) {
        APP.with(|a| a.borrow_mut().tray_available = false);
        return false;
    }
    let data = tray_data();
    let available = unsafe {
        Shell_NotifyIconW(NIM_MODIFY, &data).as_bool()
            || Shell_NotifyIconW(NIM_ADD, &data).as_bool()
    };
    APP.with(|a| a.borrow_mut().tray_available = available);
    available
}

fn toggle() {
    match audio::toggle() {
        Ok(muted) => {
            update_status();
            let mut data = tray_data();
            data.uFlags = NIF_INFO;
            data.dwInfoFlags = NIIF_INFO | NIIF_NOSOUND;
            let title = wide("MicKey");
            data.szInfoTitle[..title.len()].copy_from_slice(&title);
            let text = wide(if muted {
                "ปิดไมค์แล้ว — Muted"
            } else {
                "เปิดไมค์แล้ว — On"
            });
            data.szInfo[..text.len()].copy_from_slice(&text);
            unsafe {
                let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
            }
        }
        Err(e) => message(
            &format!("ปิด/เปิดไมค์ไม่สำเร็จ\nเลือกไมค์เริ่มต้นใน Windows > Settings > System > Sound\n\n{e}"),
            true,
        ),
    }
}

unsafe extern "system" fn keyboard_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, wp, lp) };
    }
    let event = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
    if event.flags.0 & LLKHF_INJECTED.0 != 0 {
        return unsafe { CallNextHookEx(None, code, wp, lp) };
    }
    let pressed = wp.0 as u32 == WM_KEYDOWN || wp.0 as u32 == WM_SYSKEYDOWN;
    let vk = event.vkCode as u16;
    let (hwnd, post, swallow) = APP.with(|a| {
        let mut a = a.borrow_mut();
        // Native startup checks must not act on the user's live keystrokes.
        if a.testing {
            return (a.hwnd, None, false);
        }
        if a.recording {
            a.keys.observe(vk, pressed);
            if pressed {
                if a.candidate.is_none()
                    || modifier(vk) == 0
                    || a.candidate.is_some_and(|s| modifier(s.key) != 0)
                {
                    a.candidate = Some(Shortcut {
                        key: vk,
                        mods: a.keys.mods() & !modifier(vk),
                    });
                }
            } else if a.candidate.is_some_and(|s| s.key == vk) {
                a.pending = a.candidate.take().unwrap();
                a.recording = false;
                return (a.hwnd, Some(CAPTURED), true);
            }
            (a.hwnd, None, true)
        } else if a.visible {
            a.keys.observe(vk, pressed);
            (a.hwnd, None, false)
        } else {
            let shortcut = a.shortcut;
            let fire = a.keys.event(shortcut, vk, pressed);
            (a.hwnd, if fire { Some(TOGGLE) } else { None }, false)
        }
    });
    if let Some(msg) = post {
        unsafe {
            let _ = PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(0));
        }
    }
    if swallow {
        LRESULT(1)
    } else {
        unsafe { CallNextHookEx(None, code, wp, lp) }
    }
}

unsafe extern "system" fn mouse_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // Ctrl+click and Ctrl+wheel should keep their normal meaning without toggling.
    if code >= 0
        && matches!(
            wp.0 as u32,
            WM_LBUTTONDOWN
                | WM_RBUTTONDOWN
                | WM_MBUTTONDOWN
                | WM_XBUTTONDOWN
                | WM_MOUSEWHEEL
                | WM_MOUSEHWHEEL
        )
    {
        APP.with(|a| a.borrow_mut().keys.cancel());
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

fn popup(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let status = match audio::muted() {
            Ok(true) => "เปิดไมค์ (ขณะนี้ปิดเสียง)",
            Ok(false) => "ปิดไมค์ (ขณะนี้เปิดเสียง)",
            Err(_) => "สลับปิด/เปิดไมค์",
        };
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            TOGGLE as usize,
            PCWSTR(wide(status).as_ptr()),
        );
        let _ = AppendMenuW(menu, MF_STRING, SETTING, w!("ตั้งค่าปุ่มลัด…"));
        let _ = AppendMenuW(menu, MF_STRING, CHECK_UPDATES, w!("ตรวจสอบอัปเดต…"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, QUIT, w!("ออกจากโปรแกรม"));
        let mut pos = POINT::default();
        let _ = GetCursorPos(&mut pos);
        let _ = SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            pos.x,
            pos.y,
            None,
            hwnd,
            None,
        )
        .0 as usize;
        let _ = DestroyMenu(menu);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        match command {
            SETTING => show_settings(),
            CHECK_UPDATES => updates::start(hwnd, QUIT),
            QUIT => {
                let _ = DestroyWindow(hwnd);
            }
            c if c == TOGGLE as usize => toggle(),
            _ => {}
        }
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            match wp.0 & 0xffff {
                CHOOSE => {
                    let field = APP.with(|a| {
                        let mut a = a.borrow_mut();
                        a.recording = true;
                        a.candidate = None;
                        a.keys.cancel();
                        a.field
                    });
                    set_text(field, "กดปุ่มหรือปุ่มผสมที่ต้องการ แล้วปล่อย…");
                }
                SAVE => {
                    let (path, pending, recording) = APP.with(|a| {
                        let a = a.borrow();
                        (a.path.clone(), a.pending, a.recording)
                    });
                    if recording {
                        message("กรุณากดปุ่มที่ต้องการก่อน หรือกดยกเลิก", false);
                    } else if let Err(e) = save(&path, pending) {
                        message(&format!("บันทึกไม่ได้: {e}"), true);
                    } else {
                        APP.with(|a| a.borrow_mut().shortcut = pending);
                        unsafe {
                            let _ = Shell_NotifyIconW(NIM_MODIFY, &tray_data());
                        }
                        hide_settings();
                    }
                }
                CANCEL => hide_settings(),
                CHECK_UPDATES => updates::start(hwnd, QUIT),
                QUIT => unsafe {
                    let _ = DestroyWindow(hwnd);
                },
                _ => {}
            }
            LRESULT(0)
        }
        CAPTURED => {
            update_field();
            LRESULT(0)
        }
        audio::VOLUME_CHANGED => {
            update_status();
            LRESULT(0)
        }
        audio::DEVICE_CHANGED => {
            MONITOR.with(|monitor| {
                if let Some(monitor) = monitor.borrow_mut().as_mut() {
                    let _ = monitor.rebind();
                }
            });
            update_status();
            LRESULT(0)
        }
        SHOW_SETTINGS => {
            show_settings();
            LRESULT(0)
        }
        TOGGLE => {
            toggle();
            LRESULT(0)
        }
        TRAY => {
            match lp.0 as u32 {
                WM_RBUTTONUP => popup(hwnd),
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => show_settings(),
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // X and Alt+F4 keep the keyboard hook and audio monitor alive.
            // If the shell has no tray, leave a minimized taskbar window instead.
            hide_settings();
            LRESULT(0)
        }
        WM_TIMER if wp.0 == TRAY_RETRY_TIMER => {
            if ensure_tray() {
                unsafe {
                    let _ = KillTimer(Some(hwnd), TRAY_RETRY_TIMER);
                }
                let visible = APP.with(|a| a.borrow().visible);
                if !visible {
                    unsafe {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    }
                }
            } else {
                let fallback = APP.with(|a| {
                    let mut a = a.borrow_mut();
                    a.tray_retry_count = a.tray_retry_count.saturating_add(1);
                    !a.visible && a.tray_retry_count == TRAY_FALLBACK_ATTEMPTS
                });
                if fallback {
                    unsafe {
                        let _ = ShowWindow(hwnd, SW_MINIMIZE);
                    }
                }
            }
            LRESULT(0)
        }
        WM_WINDOWPOSCHANGING if lp.0 != 0 => {
            // Display and DPI changes can cause Windows to show an otherwise
            // hidden top-level window. Only an explicit settings request should
            // reveal the tray window; preserve the taskbar fallback if no tray.
            let keep_hidden = APP.with(|a| {
                let a = a.borrow();
                !a.visible && a.tray_available
            });
            if keep_hidden {
                let pos = unsafe { &mut *(lp.0 as *mut WINDOWPOS) };
                pos.flags &= !SWP_SHOWWINDOW;
            }
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        WM_SIZE if wp.0 as u32 == SIZE_MINIMIZED => {
            let tray = APP.with(|a| {
                let mut a = a.borrow_mut();
                a.visible = false;
                a.recording = false;
                a.candidate = None;
                a.keys.cancel();
                a.tray_available
            });
            if tray {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            LRESULT(0)
        }
        WM_SYSCOMMAND if wp.0 as u32 & 0xfff0 == SC_RESTORE => {
            let allow_restore = APP.with(|a| {
                let a = a.borrow();
                a.visible || !a.tray_available
            });
            if allow_restore {
                show_settings();
            }
            LRESULT(0)
        }
        WM_ACTIVATE if wp.0 & 0xffff == WA_INACTIVE as usize => {
            APP.with(|a| {
                let mut a = a.borrow_mut();
                a.recording = false;
                a.candidate = None;
                a.keys.cancel();
            });
            update_field();
            LRESULT(0)
        }
        WM_ENDSESSION => {
            if wp.0 != 0 {
                unsafe {
                    PostQuitMessage(0);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => {
            let taskbar = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
            if taskbar != 0 && msg == taskbar {
                update_status();
                APP.with(|a| a.borrow_mut().tray_available = false);
                let available = ensure_tray();
                let visible = APP.with(|a| a.borrow().visible);
                if !visible {
                    unsafe {
                        let _ = ShowWindow(hwnd, SW_HIDE);
                    }
                }
                if !available {
                    retry_tray(hwnd);
                }
                LRESULT(0)
            } else {
                unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
            }
        }
    }
}

fn child(
    parent: HWND,
    class: PCWSTR,
    text: &str,
    style: WINDOW_STYLE,
    rect: (i32, i32, i32, i32),
    id: usize,
) -> Result<HWND> {
    unsafe {
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR(wide(text).as_ptr()),
            WS_CHILD | WS_VISIBLE | style,
            rect.0,
            rect.1,
            rect.2,
            rect.3,
            Some(parent),
            Some(HMENU(id as *mut _)),
            None,
            None,
        )?;
        let font = GetStockObject(DEFAULT_GUI_FONT);
        SendMessageW(
            hwnd,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
        Ok(hwnd)
    }
}

fn run(smoke: bool, require_tray: bool) -> Result<()> {
    unsafe {
        if smoke {
            eprintln!("Creating native window");
        }
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: w!("MicKey.Native.Window.v1"),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut _),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(windows::core::Error::from_thread());
        }
        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            class.lpszClassName,
            w!("MicKey — ตั้งค่าปุ่มปิด/เปิดไมค์"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            570,
            350,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        APP.with(|a| a.borrow_mut().hwnd = hwnd);
        child(
            hwnd,
            w!("STATIC"),
            "กดปุ่มลัดครั้งหนึ่งเพื่อปิดไมค์ กดอีกครั้งเพื่อเปิด",
            WINDOW_STYLE(0),
            (22, 16, 520, 24),
            0,
        )?;
        let field = child(hwnd, w!("STATIC"), "", WS_BORDER, (22, 52, 342, 32), 0)?;
        child(
            hwnd,
            w!("BUTTON"),
            "เลือกปุ่มลัด…",
            WS_TABSTOP,
            (378, 51, 150, 34),
            CHOOSE,
        )?;
        child(
            hwnd,
            w!("STATIC"),
            "เลือกปุ่มเดียวหรือปุ่มผสม เช่น F8 หรือ Ctrl + Shift + M\nกด X เพื่อซ่อนไป System Tray และใช้ปุ่มลัดต่อได้",
            WINDOW_STYLE(0),
            (22, 99, 510, 44),
            0,
        )?;
        let status = child(
            hwnd,
            w!("STATIC"),
            "",
            WINDOW_STYLE(0),
            (22, 157, 510, 42),
            0,
        )?;
        child(
            hwnd,
            w!("BUTTON"),
            "ออกจากโปรแกรม",
            WS_TABSTOP,
            (22, 212, 174, 34),
            QUIT,
        )?;
        child(
            hwnd,
            w!("BUTTON"),
            "บันทึกและซ่อน",
            WS_TABSTOP,
            (220, 212, 160, 34),
            SAVE,
        )?;
        child(
            hwnd,
            w!("BUTTON"),
            "ซ่อนหน้าต่าง",
            WS_TABSTOP,
            (392, 212, 136, 34),
            CANCEL,
        )?;
        child(
            hwnd,
            w!("BUTTON"),
            "ตรวจสอบอัปเดต…",
            WS_TABSTOP,
            (22, 256, 174, 30),
            CHECK_UPDATES,
        )?;
        child(
            hwnd,
            w!("STATIC"),
            &format!("MicKey {}", updates::VERSION),
            WINDOW_STYLE(0),
            (220, 264, 308, 22),
            0,
        )?;
        APP.with(|a| {
            let mut a = a.borrow_mut();
            a.field = field;
            a.status = status;
        });
        let icons = icons::Icons::new()?;
        APP.with(|a| a.borrow_mut().icons = Some(icons));
        MONITOR.with(|monitor| -> Result<()> {
            *monitor.borrow_mut() = Some(audio::Monitor::new(hwnd)?);
            Ok(())
        })?;
        update_status();
        if smoke {
            eprintln!("Status icons and Core Audio event monitor initialized");
        }
        if smoke {
            eprintln!("Installing input hooks");
        }
        let keyboard = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_hook),
            Some(instance.into()),
            0,
        )?;
        let mouse = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(instance.into()), 0)
        {
            Ok(hook) => hook,
            Err(e) => {
                let _ = UnhookWindowsHookEx(keyboard);
                return Err(e);
            }
        };
        if smoke {
            eprintln!("Adding tray icon");
        }
        let tray_available = ensure_tray();
        if smoke && !tray_available {
            eprintln!("Tray unavailable; normal mode will use the taskbar fallback");
        }
        let path = APP.with(|a| a.borrow().path.clone());
        let mut close_test_error = None;
        if smoke {
            APP.with(|a| a.borrow_mut().visible = true);
            SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
            let alive = IsWindow(Some(hwnd)).as_bool();
            let background = APP.with(|a| !a.borrow().visible);
            let has_tray = APP.with(|a| a.borrow().tray_available);
            let hidden = !IsWindowVisible(hwnd).as_bool();
            eprintln!(
                "Close-to-tray: window_alive={alive}, hotkeys_enabled={background}, tray={has_tray}, hidden={hidden}"
            );
            if !alive || !background || (has_tray && !hidden) || (require_tray && !has_tray) {
                close_test_error = Some(windows::core::Error::new(
                    E_FAIL,
                    "Close-to-tray check failed",
                ));
            }
            if has_tray {
                APP.with(|a| a.borrow_mut().forced_tray_failure = true);
                let taskbar = RegisterWindowMessageW(w!("TaskbarCreated"));
                SendMessageW(hwnd, taskbar, Some(WPARAM(0)), Some(LPARAM(0)));
                SendMessageW(
                    hwnd,
                    WM_TIMER,
                    Some(WPARAM(TRAY_RETRY_TIMER)),
                    Some(LPARAM(0)),
                );
                let hidden_during_failure = !IsWindowVisible(hwnd).as_bool();
                for _ in 1..TRAY_FALLBACK_ATTEMPTS {
                    SendMessageW(
                        hwnd,
                        WM_TIMER,
                        Some(WPARAM(TRAY_RETRY_TIMER)),
                        Some(LPARAM(0)),
                    );
                }
                let taskbar_fallback = IsIconic(hwnd).as_bool() && IsWindowVisible(hwnd).as_bool();
                APP.with(|a| a.borrow_mut().forced_tray_failure = false);
                SendMessageW(
                    hwnd,
                    WM_TIMER,
                    Some(WPARAM(TRAY_RETRY_TIMER)),
                    Some(LPARAM(0)),
                );
                let tray_recovered =
                    APP.with(|a| a.borrow().tray_available) && !IsWindowVisible(hwnd).as_bool();
                eprintln!(
                    "Tray restart: hidden_during_failure={hidden_during_failure}, taskbar_fallback={taskbar_fallback}, recovered={tray_recovered}"
                );
                if !hidden_during_failure || !taskbar_fallback || !tray_recovered {
                    close_test_error = Some(windows::core::Error::new(
                        E_FAIL,
                        "Tray recovery check failed",
                    ));
                }
                SendMessageW(hwnd, WM_DISPLAYCHANGE, Some(WPARAM(32)), Some(LPARAM(0)));
                SendMessageW(
                    hwnd,
                    WM_SYSCOMMAND,
                    Some(WPARAM(SC_RESTORE as usize)),
                    Some(LPARAM(0)),
                );
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let stayed_hidden =
                    !IsWindowVisible(hwnd).as_bool() && APP.with(|a| !a.borrow().visible);
                eprintln!("Display change: stayed_hidden={stayed_hidden}");
                show_settings();
                let restored = IsWindowVisible(hwnd).as_bool() && APP.with(|a| a.borrow().visible);
                SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
                let closed = !IsWindowVisible(hwnd).as_bool() && APP.with(|a| !a.borrow().visible);
                eprintln!("Settings: restored={restored}, close_to_tray={closed}");
                if !stayed_hidden || !restored || !closed {
                    close_test_error = Some(windows::core::Error::new(
                        E_FAIL,
                        "Display change/window visibility check failed",
                    ));
                }
            }
            eprintln!("Closing test window");
            SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(QUIT)), Some(LPARAM(0)));
            if IsWindow(Some(hwnd)).as_bool() {
                close_test_error = Some(windows::core::Error::new(E_FAIL, "Exit command failed"));
            }
        } else if !tray_available || !path.exists() || load(&path).is_err() {
            show_settings();
        }
        let mut msg = MSG::default();
        let result = loop {
            let value = GetMessageW(&mut msg, None, 0, 0).0;
            if value == -1 {
                break Err(windows::core::Error::from_thread());
            }
            if value == 0 {
                break Ok(());
            }
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        };
        let _ = UnhookWindowsHookEx(keyboard);
        let _ = UnhookWindowsHookEx(mouse);
        let _ = Shell_NotifyIconW(NIM_DELETE, &tray_data());
        MONITOR.with(|monitor| {
            monitor.borrow_mut().take();
        });
        APP.with(|a| {
            a.borrow_mut().icons.take();
        });
        if smoke {
            eprintln!("Startup smoke test complete");
        }
        result.and_then(|()| match close_test_error {
            Some(error) => Err(error),
            None => Ok(()),
        })
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let require_tray = args.iter().any(|s| s == "--tray-test");
    let smoke = require_tray || args.iter().any(|s| s == "--smoke-test");
    APP.with(|a| a.borrow_mut().testing = smoke);
    if let Some(index) = args.iter().position(|s| s == "--export-icons") {
        let result = args
            .get(index + 1)
            .ok_or_else(|| std::io::Error::other("Expected output folder"))
            .and_then(|folder| icons::export(std::path::Path::new(folder)));
        if let Err(e) = result {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    unsafe {
        if let Err(e) = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() {
            message(&format!("Cannot initialize audio: {e}"), true);
            std::process::exit(1);
        }
        if args.iter().any(|s| s == "--status") {
            let ok = match audio::muted() {
                Ok(muted) => {
                    println!("Default input muted: {muted}");
                    true
                }
                Err(e) => {
                    eprintln!("Audio unavailable: {e}");
                    false
                }
            };
            CoUninitialize();
            if !ok {
                std::process::exit(1);
            }
            return;
        }
        let mutex_name = if smoke {
            wide(&format!("Local\\MicKey.Native.Test.{}", std::process::id()))
        } else {
            wide("Local\\MicKey.Native.Singleton.v1")
        };
        let mutex = match CreateMutexW(None, false, PCWSTR(mutex_name.as_ptr())) {
            Ok(handle) => handle,
            Err(e) => {
                message(&format!("Cannot create instance lock: {e}"), true);
                std::process::exit(1);
            }
        };
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if smoke {
                eprintln!("MicKey is already running; close it before running startup tests.");
                let _ = CloseHandle(mutex);
                CoUninitialize();
                std::process::exit(2);
            }
            if let Ok(hwnd) = FindWindowW(w!("MicKey.Native.Window.v1"), None) {
                let _ = PostMessageW(Some(hwnd), SHOW_SETTINGS, WPARAM(0), LPARAM(0));
            }
            let _ = CloseHandle(mutex);
            CoUninitialize();
            return;
        }
        let path = match config_path() {
            Ok(path) => path,
            Err(e) => {
                message(&e, true);
                std::process::exit(1);
            }
        };
        let loaded = load(&path);
        let shortcut = loaded.as_ref().ok().copied().flatten().unwrap_or_default();
        APP.with(|a| {
            let mut a = a.borrow_mut();
            a.path = path;
            a.shortcut = shortcut;
            a.pending = shortcut;
        });
        if let Err(e) = loaded {
            if !smoke {
                message(&e, true);
            }
        }
        let result = run(smoke, require_tray);
        MONITOR.with(|monitor| {
            monitor.borrow_mut().take();
        });
        APP.with(|a| {
            a.borrow_mut().icons.take();
        });
        let _ = CloseHandle(mutex);
        CoUninitialize();
        if let Err(e) = result {
            if smoke {
                eprintln!("MicKey could not start: {e}");
            } else {
                message(&format!("MicKey could not start: {e}"), true);
            }
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;
    #[test]
    fn save_and_replace_settings_round_trip() {
        let folder =
            std::env::temp_dir().join(format!("mickey-config-test-{}", std::process::id()));
        let path = folder.join("shortcut.txt");
        let first = Shortcut { key: 0x77, mods: 0 };
        save(&path, first).unwrap();
        assert_eq!(load(&path).unwrap(), Some(first));
        let second = Shortcut {
            key: 0x4d,
            mods: keys::CTRL | keys::SHIFT,
        };
        save(&path, second).unwrap();
        assert_eq!(load(&path).unwrap(), Some(second));
        assert!(!path.with_extension("tmp").exists());
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&folder).unwrap();
    }
    #[test]
    fn shortcut_rejects_invalid_key_and_modifier_bits() {
        assert!(!Shortcut { key: 0, mods: 0 }.valid());
        assert!(
            !Shortcut {
                key: 0x77,
                mods: 16
            }
            .valid()
        );
        assert!(
            !Shortcut {
                key: 0xa2,
                mods: keys::CTRL
            }
            .valid()
        );
        assert!(
            Shortcut {
                key: 0x4d,
                mods: keys::CTRL | keys::ALT
            }
            .valid()
        );
    }
}
