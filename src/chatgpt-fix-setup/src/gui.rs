//! Minimal Win32 GUI install wizard for Setup (parity with the initial
//! Codex-NTFS-Fix .NET WinForms installer: a real guided window with
//! description, status text, a progress bar and Install/Cancel buttons).
//! Pure FFI — no external crates, matching the project's std-only constraint.
//!
//! Layout:
//!   [ ChatGPT-Fix 安装程序 ]          (window title)
//!   ChatGPT-Fix 1.0.5                (product label)
//!   将把官方 ChatGPT 桌面版复制到用户目录运行…… (guide text)
//!   [ status text ]                  (status label)
//!   [==============progress========] (progress bar)
//!   [ 安装 ]  [ 取消 ]               (buttons)

use std::os::raw::{c_int, c_void};
use std::ptr;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

type Hwnd = *mut c_void;
type Hinstance = *mut c_void;
type Wparam = usize;
type Lparam = isize;
type Lresult = isize;

const WM_CREATE: u32 = 0x0001;
const WM_COMMAND: u32 = 0x0111;
const WM_CLOSE: u32 = 0x0010;
const WM_DESTROY: u32 = 0x0002;
const WM_APP_PROGRESS: u32 = 0x8000 + 1;
const WM_APP_DONE: u32 = 0x8000 + 2;
const WM_APP_FAIL: u32 = 0x8000 + 3;

const CW_USEDEFAULT: c_int = -1;
const WS_OVERLAPPEDWINDOW: u32 = 0x00CF0000;
const WS_VISIBLE: u32 = 0x10000000;
const WS_CHILD: u32 = 0x40000000;
const WS_TABSTOP: u32 = 0x00010000;
const BS_PUSHBUTTON: u32 = 0x00000000;
const SS_LEFT: u32 = 0x00000000;
const PBS_SMOOTH: u32 = 0x01;

const ID_BUTTON_INSTALL: c_int = 100;
const ID_BUTTON_CANCEL: c_int = 101;

static HWND_MAIN: AtomicUsize = AtomicUsize::new(0);
static HWND_PROGRESS: AtomicUsize = AtomicUsize::new(0);
static HWND_STATUS: AtomicUsize = AtomicUsize::new(0);
static HWND_BUTTON_INSTALL: AtomicUsize = AtomicUsize::new(0);
static INSTALLING: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" {
    fn RegisterClassExW(lpwcx: *const WndClassExW) -> u16;
    fn CreateWindowExW(
        dwExStyle: u32,
        lpClassName: *const u16,
        lpWindowName: *const u16,
        dwStyle: u32,
        x: c_int,
        y: c_int,
        nWidth: c_int,
        nHeight: c_int,
        hWndParent: Hwnd,
        hMenu: Hwnd,
        hInstance: Hinstance,
        lpParam: *mut c_void,
    ) -> Hwnd;
    fn DefWindowProcW(hWnd: Hwnd, msg: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
    fn ShowWindow(hWnd: Hwnd, nCmdShow: c_int) -> c_int;
    fn UpdateWindow(hWnd: Hwnd) -> c_int;
    fn DestroyWindow(hWnd: Hwnd) -> c_int;
    fn PostQuitMessage(nExitCode: c_int);
    fn GetMessageW(lpMsg: *mut Msg, hWnd: Hwnd, wMsgFilterMin: u32, wMsgFilterMax: u32) -> c_int;
    fn TranslateMessage(lpMsg: *const Msg) -> c_int;
    fn DispatchMessageW(lpMsg: *const Msg) -> Lresult;
    fn GetModuleHandleW(lpModuleName: *const u16) -> Hinstance;
    fn LoadCursorW(hInstance: Hinstance, lpCursorName: *const u16) -> Hwnd;
    fn LoadIconW(hInstance: Hinstance, lpIconName: *const u16) -> Hwnd;
    fn SendMessageW(hWnd: Hwnd, msg: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
    fn PostMessageW(hWnd: Hwnd, msg: u32, wparam: Wparam, lparam: Lparam) -> c_int;
    fn SetWindowTextW(hWnd: Hwnd, lpString: *const u16) -> c_int;
    fn EnableWindow(hWnd: Hwnd, bEnable: c_int) -> c_int;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn IsUserAnAdmin() -> c_int;
    fn ShellExecuteW(
        hwnd: Hwnd,
        lpOperation: *const u16,
        lpFile: *const u16,
        lpParameters: *const u16,
        lpDirectory: *const u16,
        nShowCmd: c_int,
    ) -> Hinstance;
}

/// True when the current process runs elevated (admin).
fn is_admin() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

/// Relaunch this EXE with the "runas" verb (UAC elevation) and exit the
/// current non-elevated process. This mirrors the initial installer, which
/// ran as administrator because copying the official package out of
/// WindowsApps requires directory access privileges (the initial logs show
/// FAIL_UNAUTHORIZEDACCESSEXCEPTION before elevation succeeded).
///
/// Returns `true` when the elevated process was launched, `false` when the
/// user cancelled the UAC prompt or elevation failed.
fn elevate_and_restart() -> bool {
    let exe = std::env::current_exe().unwrap_or_default();
    let exe_w = to_w(&exe.to_string_lossy());
    let args = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let args_w = to_w(&args);
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            to_w("runas").as_ptr(),
            exe_w.as_ptr(),
            args_w.as_ptr(),
            ptr::null(),
            5, // SW_SHOWNORMAL
        )
    };
    // ShellExecuteW returns a value > 32 on success.
    (result as usize) > 32
}

const PBM_SETRANGE: u32 = 0x0401;
const PBM_SETPOS: u32 = 0x0402;
const IDI_APPLICATION: *const u16 = 0x7F00usize as *const u16;
const IDC_ARROW: *const u16 = 0x7F00usize as *const u16;

#[repr(C)]
struct WndClassExW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: Option<unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam) -> Lresult>,
    cb_cls_extra: c_int,
    cb_wnd_extra: c_int,
    h_instance: Hinstance,
    h_icon: Hwnd,
    h_cursor: Hwnd,
    hbr_background: Hwnd,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm: Hwnd,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
    time: u32,
    pt_x: c_int,
    pt_y: c_int,
}

fn to_w(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

#[allow(clippy::too_many_arguments)]
fn create_child(
    hwnd: Hwnd,
    class: *const u16,
    text: *const u16,
    style: u32,
    x: c_int,
    y: c_int,
    w: c_int,
    h: c_int,
    id: Hwnd,
) -> Hwnd {
    unsafe {
        CreateWindowExW(
            0,
            class,
            text,
            WS_CHILD | WS_VISIBLE | style,
            x,
            y,
            w,
            h,
            hwnd,
            id,
            GetModuleHandleW(ptr::null()),
            ptr::null_mut(),
        )
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: Hwnd,
    msg: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    unsafe {
        match msg {
            WM_CREATE => {
                HWND_MAIN.store(hwnd as usize, Ordering::SeqCst);
                // Product title
                create_child(
                    hwnd,
                    to_w("STATIC").as_ptr(),
                    to_w("ChatGPT-Fix 安装程序").as_ptr(),
                    SS_LEFT,
                    20,
                    14,
                    380,
                    24,
                    ptr::null_mut(),
                );
                // Guide text (multi-line explanation)
                create_child(
                    hwnd,
                    to_w("STATIC").as_ptr(),
                    to_w(
                        "本安装将把官方 ChatGPT 桌面版复制到用户目录运行，\n修复 NTFS 内核非分页池泄漏，并创建开始菜单快捷方式。\n不会修改 WindowsApps 中的官方包。",
                    )
                    .as_ptr(),
                    SS_LEFT,
                    20,
                    40,
                    380,
                    60,
                    ptr::null_mut(),
                );
                // Status label
                let status = create_child(
                    hwnd,
                    to_w("STATIC").as_ptr(),
                    to_w("准备就绪，点击“安装”开始。").as_ptr(),
                    SS_LEFT,
                    20,
                    108,
                    380,
                    20,
                    ptr::null_mut(),
                );
                HWND_STATUS.store(status as usize, Ordering::SeqCst);
                // Progress bar
                let progress = create_child(
                    hwnd,
                    to_w("msctls_progress32").as_ptr(),
                    ptr::null(),
                    PBS_SMOOTH,
                    20,
                    134,
                    380,
                    20,
                    ptr::null_mut(),
                );
                HWND_PROGRESS.store(progress as usize, Ordering::SeqCst);
                SendMessageW(progress, PBM_SETRANGE, 0, 100);
                // Buttons: Install / Cancel
                let install_btn = create_child(
                    hwnd,
                    to_w("BUTTON").as_ptr(),
                    to_w("安装").as_ptr(),
                    WS_TABSTOP | BS_PUSHBUTTON,
                    170,
                    170,
                    90,
                    30,
                    (ID_BUTTON_INSTALL as usize) as Hwnd,
                );
                HWND_BUTTON_INSTALL.store(install_btn as usize, Ordering::SeqCst);
                create_child(
                    hwnd,
                    to_w("BUTTON").as_ptr(),
                    to_w("取消").as_ptr(),
                    WS_TABSTOP | BS_PUSHBUTTON,
                    270,
                    170,
                    90,
                    30,
                    (ID_BUTTON_CANCEL as usize) as Hwnd,
                );
                0
            }
            WM_COMMAND => {
                let id = (wparam & 0xFFFF) as c_int;
                if id == ID_BUTTON_INSTALL && INSTALLING.load(Ordering::SeqCst) == 0 {
                    start_install(hwnd);
                } else if id == ID_BUTTON_CANCEL && INSTALLING.load(Ordering::SeqCst) == 0 {
                    DestroyWindow(hwnd);
                }
                0
            }
            WM_APP_PROGRESS => {
                let progress = HWND_PROGRESS.load(Ordering::SeqCst) as Hwnd;
                if !progress.is_null() {
                    SendMessageW(progress, PBM_SETPOS, wparam, 0);
                }
                0
            }
            WM_APP_DONE => {
                let status = HWND_STATUS.load(Ordering::SeqCst) as Hwnd;
                if !status.is_null() {
                    SetWindowTextW(
                        status,
                        to_w("安装完成！可从开始菜单启动 ChatGPT。").as_ptr(),
                    );
                }
                let install_btn = HWND_BUTTON_INSTALL.load(Ordering::SeqCst) as Hwnd;
                if !install_btn.is_null() {
                    SetWindowTextW(install_btn, to_w("关闭").as_ptr());
                }
                INSTALLING.store(0, Ordering::SeqCst);
                0
            }
            WM_APP_FAIL => {
                let status = HWND_STATUS.load(Ordering::SeqCst) as Hwnd;
                if !status.is_null() {
                    SetWindowTextW(
                        status,
                        to_w("安装失败，详见 logs\\setup.jsonl。可再次点击“安装”重试。").as_ptr(),
                    );
                }
                let install_btn = HWND_BUTTON_INSTALL.load(Ordering::SeqCst) as Hwnd;
                if !install_btn.is_null() {
                    EnableWindow(install_btn, 1);
                }
                INSTALLING.store(0, Ordering::SeqCst);
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn start_install(hwnd: Hwnd) {
    INSTALLING.store(1, Ordering::SeqCst);
    let status = HWND_STATUS.load(Ordering::SeqCst) as Hwnd;
    if !status.is_null() {
        unsafe {
            SetWindowTextW(
                status,
                to_w("正在探测官方包并复制到用户目录（约 1-2 分钟）…").as_ptr(),
            );
        }
    }
    let install_btn = HWND_BUTTON_INSTALL.load(Ordering::SeqCst) as Hwnd;
    if !install_btn.is_null() {
        unsafe {
            EnableWindow(install_btn, 0);
        }
    }
    // Raw handles are not Send; pass the main-window handle as usize so the
    // worker thread can post progress messages back.
    let hwnd_usize = hwnd as usize;
    let source_dir = match std::env::current_exe() {
        Ok(exe) => exe.parent().map(|p| p.to_path_buf()),
        Err(_) => None,
    };
    std::thread::spawn(move || {
        let main_hwnd = hwnd_usize as Hwnd;
        let Some(source) = source_dir else {
            unsafe { PostMessageW(main_hwnd, WM_APP_FAIL, 0, 0) };
            return;
        };
        // Map the byte progress into 8..100% of the bar (first few percent
        // are package discovery / pointer writing).
        let progress_fn = |done: u64, total: u64| {
            #[allow(clippy::manual_checked_ops)]
            let pct = if total > 0 {
                8 + (done * 90 / total) as usize
            } else {
                8
            };
            unsafe { PostMessageW(main_hwnd, WM_APP_PROGRESS, pct, 0) };
        };
        let code = super::run_install_with_progress(&source, &progress_fn);
        let msg = if code == std::process::ExitCode::SUCCESS {
            WM_APP_DONE
        } else {
            WM_APP_FAIL
        };
        unsafe { PostMessageW(main_hwnd, msg, 0, 0) };
    });
}

/// Entry point: run the GUI install wizard (blocking message loop).
pub fn run_gui_install() -> std::process::ExitCode {
    // Elevation first (parity with the initial installer, which ran as
    // administrator): without admin rights copying the official package out
    // of WindowsApps fails with an access-denied error.
    // CHATGPT_FIX_SETUP_NO_ELEVATE=1 skips elevation (tests / explicit).
    if !is_admin()
        && std::env::var_os("CHATGPT_FIX_SETUP_NO_ELEVATE").is_none()
        && elevate_and_restart()
    {
        return std::process::ExitCode::SUCCESS;
        // UAC cancelled or failed; continue in non-elevated mode.
        // The GUI will show a warning but the user can still retry.
    }
    unsafe {
        let hinst = GetModuleHandleW(ptr::null());
        let class_name = to_w("ChatGPTFixSetupWizard");
        let wc = WndClassExW {
            cb_size: std::mem::size_of::<WndClassExW>() as u32,
            style: 0,
            lpfn_wnd_proc: Some(wnd_proc),
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: hinst,
            h_icon: LoadIconW(ptr::null_mut(), IDI_APPLICATION),
            h_cursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            hbr_background: ptr::null_mut(),
            lpsz_menu_name: ptr::null(),
            lpsz_class_name: class_name.as_ptr(),
            h_icon_sm: ptr::null_mut(),
        };
        RegisterClassExW(&wc);
        let title = to_w("ChatGPT-Fix 安装程序");
        let window = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            420,
            240,
            ptr::null_mut(),
            ptr::null_mut(),
            hinst,
            ptr::null_mut(),
        );
        if window.is_null() {
            return std::process::ExitCode::from(4);
        }
        ShowWindow(window, 5); // SW_SHOW
        UpdateWindow(window);
        let mut msg = Msg {
            hwnd: ptr::null_mut(),
            message: 0,
            wparam: 0,
            lparam: 0,
            time: 0,
            pt_x: 0,
            pt_y: 0,
        };
        while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    std::process::ExitCode::SUCCESS
}
