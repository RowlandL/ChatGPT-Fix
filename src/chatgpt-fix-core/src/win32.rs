//! Minimal Win32 interop for the launcher (no external crates; raw FFI).
//!
//! Scope (windows target only):
//! - `activate_chatgpt_window`: bring an already-running ChatGPT window to
//!   the foreground when the single-instance guard reuses it. The AUMID
//!   activation (`shell:AppsFolder\...`) only works for MSIX-registered
//!   apps; a user-owned baseline copy is not registered, so we enumerate
//!   top-level windows instead.

use std::os::raw::{c_int, c_void};
use std::ptr;
use std::sync::Mutex;

type Handle = *mut c_void;
type Bool = c_int;
type Dword = u32;
type Hwnd = Handle;

const SW_RESTORE: c_int = 9;
const PROCESS_SET_QUOTA: Dword = 0x0100;
const PROCESS_TERMINATE: Dword = 0x0001;
const SYNCHRONIZE: Dword = 0x0010_0000;
const INFINITE: Dword = 0xFFFF_FFFF;

#[link(name = "user32")]
unsafe extern "system" {
    fn EnumWindows(
        lpEnumFunc: Option<unsafe extern "system" fn(Hwnd, *mut c_void) -> Bool>,
        lParam: *mut c_void,
    ) -> Bool;
    fn GetWindowTextW(hWnd: Hwnd, lpString: *mut u16, nMaxCount: c_int) -> c_int;
    fn IsWindowVisible(hWnd: Hwnd) -> Bool;
    fn SetForegroundWindow(hWnd: Hwnd) -> Bool;
    fn ShowWindow(hWnd: Hwnd, nCmdShow: c_int) -> Bool;
}

unsafe extern "system" {
    fn CreateJobObjectW(lpJobAttributes: *mut c_void, lpName: *const u16) -> Handle;
    fn AssignProcessToJobObject(hJob: Handle, hProcess: Handle) -> Bool;
    fn OpenProcess(dwDesiredAccess: Dword, bInheritHandle: Bool, dwProcessId: Dword) -> Handle;
    fn CloseHandle(hObject: Handle) -> Bool;
    fn WaitForSingleObject(hHandle: Handle, dwMilliseconds: Dword) -> Dword;
}

/// Best-effort: find a visible top-level window whose title contains
/// "ChatGPT" (case-insensitive) and restore + foreground it. Returns true
/// when a window was activated.
pub fn activate_chatgpt_window() -> bool {
    // Re-entrancy guard: EnumWindows must not be re-entered from the
    // callback; the callback only reads and records the handle.
    unsafe {
        let mut found: Hwnd = ptr::null_mut();
        let mut ctx = Ctx {
            found: &mut found as *mut Hwnd,
            stop: false,
        };
        unsafe extern "system" fn callback(hwnd: Hwnd, lparam: *mut c_void) -> Bool {
            let ctx = unsafe { &mut *(lparam as *mut Ctx) };
            if ctx.stop {
                return 0;
            }
            if unsafe { IsWindowVisible(hwnd) } == 0 {
                return 1;
            }
            let mut buf = [0u16; 128];
            let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as c_int) };
            if len <= 0 {
                return 1;
            }
            let title = String::from_utf16_lossy(&buf[..len as usize]);
            let hay = title.to_ascii_lowercase();
            if hay.contains("chatgpt") {
                unsafe {
                    *ctx.found = hwnd;
                }
                ctx.stop = true;
                return 0; // stop enumeration
            }
            1
        }
        EnumWindows(Some(callback), &mut ctx as *mut Ctx as *mut c_void);
        let hwnd = found;
        if hwnd.is_null() {
            return false;
        }
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
        true
    }
}

struct Ctx {
    found: *mut Hwnd,
    stop: bool,
}

/// Create an unnamed Job object. No limits are set and KILL_ON_JOB_CLOSE
/// is intentionally NOT enabled: the Job is an ownership/membership
/// container, never a kill switch that could take the app down if the
/// launcher exits unexpectedly.
pub fn create_job() -> Option<Handle> {
    unsafe {
        let job = CreateJobObjectW(ptr::null_mut(), ptr::null());
        if job.is_null() { None } else { Some(job) }
    }
}

/// Assign a process (by PID) into a Job. Returns true on success.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn assign_process_to_job(job: Handle, pid: u32) -> bool {
    unsafe {
        let proc = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
        if proc.is_null() {
            return false;
        }
        let ok = AssignProcessToJobObject(job, proc) != 0;
        CloseHandle(proc);
        ok
    }
}

/// Park a Job handle in a static so it stays alive for the launcher
/// process's whole lifetime (resident mode). The OS reclaims it when the
/// process exits; we never close it explicitly.
pub fn hold_job(job: Handle) {
    static HELD_JOB: Mutex<Option<usize>> = Mutex::new(None);
    if let Ok(mut guard) = HELD_JOB.lock() {
        *guard = Some(job as usize);
    }
}

/// Block until the given process exits (resident launcher mode). Returns
/// false when the process could not be opened.
pub fn wait_for_process(pid: u32) -> bool {
    unsafe {
        let proc = OpenProcess(SYNCHRONIZE, 0, pid);
        if proc.is_null() {
            return false;
        }
        WaitForSingleObject(proc, INFINITE);
        CloseHandle(proc);
        true
    }
}
