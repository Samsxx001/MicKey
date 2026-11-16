use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use windows::{
    Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*},
    core::{PCWSTR, w},
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPOSITORY: &str = match option_env!("MICKEY_REPOSITORY") {
    Some(repo) => repo,
    None => "Samsxx001/MicKey",
};
static BUSY: AtomicBool = AtomicBool::new(false);

fn powershell(script: &Path) -> Command {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let mut command =
        Command::new(PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"));
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(script)
        .creation_flags(CREATE_NO_WINDOW.0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}
fn report(hwnd: usize, text: &str) {
    let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            Some(HWND(hwnd as *mut _)),
            PCWSTR(text.as_ptr()),
            w!("MicKey"),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

pub fn start(hwnd: HWND, quit_command: usize) {
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let hwnd = hwnd.0 as usize;
    std::thread::spawn(move || {
        let result = run(hwnd, quit_command);
        BUSY.store(false, Ordering::SeqCst);
        if let Err(error) = result {
            report(hwnd, &format!("เริ่มตรวจสอบอัปเดตไม่ได้: {error}"));
        }
    });
}

fn run(hwnd: usize, quit_command: usize) -> std::result::Result<(), String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let stage = std::env::temp_dir().join(format!("MicKey-update-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let result = run_in_stage(hwnd, quit_command, &stage, stamp);
    // A successful install worker still needs this folder after the parent exits.
    if !matches!(result, Ok(true)) {
        let _ = std::fs::remove_dir_all(&stage);
    }
    result.map(|_| ())
}

fn run_in_stage(
    hwnd: usize,
    quit_command: usize,
    stage: &Path,
    stamp: u128,
) -> std::result::Result<bool, String> {
    let script = stage.join("update.ps1");
    // Windows PowerShell 5.1 needs a BOM to read the Thai UI text as UTF-8.
    let mut content = vec![0xef, 0xbb, 0xbf];
    content.extend_from_slice(
        include_str!("update.ps1")
            .trim_start_matches('\u{feff}')
            .as_bytes(),
    );
    std::fs::write(&script, content).map_err(|e| e.to_string())?;
    let status = powershell(&script)
        .args([
            "-Mode",
            "Check",
            "-Repository",
            REPOSITORY,
            "-CurrentVersion",
            VERSION,
            "-Stage",
        ])
        .arg(stage)
        .status()
        .map_err(|e| e.to_string())?;
    if status.code() != Some(10) {
        return Ok(false);
    }
    let sha = std::fs::read_to_string(stage.join("expected.sha256")).map_err(|e| e.to_string())?;
    if sha.len() != 64 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Invalid update checksum".into());
    }
    let target = std::env::current_exe().map_err(|e| e.to_string())?;
    let event_name = format!("Local\\MicKey.UpdateReady.{}.{stamp}", std::process::id());
    let event_wide: Vec<u16> = event_name.encode_utf16().chain(Some(0)).collect();
    let event = unsafe { CreateEventW(None, true, false, PCWSTR(event_wide.as_ptr())) }
        .map_err(|e| e.to_string())?;
    struct Event(HANDLE);
    impl Drop for Event {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let event = Event(event);
    let mut child = powershell(&script)
        .args(["-Mode", "Install", "-Repository", REPOSITORY, "-Stage"])
        .arg(stage)
        .arg("-Target")
        .arg(&target)
        .arg("-ParentId")
        .arg(std::process::id().to_string())
        .arg("-ReadyEvent")
        .arg(&event_name)
        .arg("-ExpectedSha")
        .arg(&sha)
        .spawn()
        .map_err(|e| e.to_string())?;
    if unsafe { WaitForSingleObject(event.0, 15000) } != WAIT_OBJECT_0 {
        let _ = child.kill();
        let _ = child.wait();
        return Err("ตัวติดตั้งยังไม่พร้อม โปรแกรมปัจจุบันจะทำงานต่อ".into());
    }
    unsafe {
        PostMessageW(
            Some(HWND(hwnd as *mut _)),
            WM_COMMAND,
            WPARAM(quit_command),
            LPARAM(0),
        )
    }
    .map_err(|e| e.to_string())?;
    Ok(true)
}
