//! Windows: native message boxes (used when there is no WebView2), x64 emulation on ARM64, free disk space, and the
//! process flags of the detached launcher (ADR 0004, 0006).

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MessageBoxW,
};

/// `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`: the launcher has no console and does not share Ctrl+C with us.
pub const DETACHED_CREATION_FLAGS: u32 = 0x0000_0008 | 0x0000_0200;
/// `CREATE_NO_WINDOW`: for `java.exe -version`, which would otherwise flash a console window.
pub const NO_WINDOW_CREATION_FLAGS: u32 = 0x0800_0000;

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain(std::iter::once(0)).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxKind {
    Info,
    Error,
}

/// Shows a modal message box and returns when the player closes it.
pub fn message_box(title: &str, text: &str, kind: BoxKind) {
    let icon = match kind {
        BoxKind::Info => MB_ICONINFORMATION,
        BoxKind::Error => MB_ICONERROR,
    };
    let title = wide(title);
    let text = wide(text);
    // SAFETY: both buffers are NUL-terminated UTF-16 that outlive the call; a null owner window is allowed.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | icon | MB_SETFOREGROUND);
    }
}

/// Bytes available to this user on the volume that holds `path` (the deepest existing ancestor is asked).
pub fn free_space(path: &Path) -> io::Result<u64> {
    let existing = path.ancestors().find(|p| p.exists()).unwrap_or(path);
    let dir = wide(&existing.to_string_lossy());
    let mut available: u64 = 0;
    // SAFETY: `dir` is NUL-terminated and `available` is a valid out pointer; the other outputs may be null.
    let ok = unsafe { GetDiskFreeSpaceExW(dir.as_ptr(), &mut available, std::ptr::null_mut(), std::ptr::null_mut()) };
    if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(available) }
}

/// Whether this Windows can run x64 code. Only meaningful for the ARM64 build: Windows 11 on ARM emulates x64,
/// Windows 10 on ARM does not. `GetMachineTypeAttributes` exists from Windows 11, so it is looked up at run time
/// (a static import would stop the exe from starting on Windows 10); without it there is no x64 emulation.
pub fn x64_emulation_available() -> bool {
    const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
    const USER_ENABLED: i32 = 0x1;
    type GetMachineTypeAttributes = unsafe extern "system" fn(u16, *mut i32) -> i32;

    let kernel32 = wide("kernel32.dll");
    // SAFETY: kernel32 is always loaded; the name is NUL-terminated.
    let module: HMODULE = unsafe { GetModuleHandleW(kernel32.as_ptr()) };
    if module.is_null() {
        return false;
    }
    // SAFETY: the symbol name is a NUL-terminated ASCII string.
    let Some(symbol) = (unsafe { GetProcAddress(module, c"GetMachineTypeAttributes".as_ptr().cast()) }) else {
        return false;
    };
    // SAFETY: the documented signature is HRESULT(USHORT Machine, MACHINE_ATTRIBUTES* MachineTypeAttributes).
    let function: GetMachineTypeAttributes = unsafe { std::mem::transmute(symbol) };
    let mut attributes: i32 = 0;
    // SAFETY: `attributes` is a valid out pointer for the call.
    let hresult = unsafe { function(IMAGE_FILE_MACHINE_AMD64, &mut attributes) };
    hresult >= 0 && attributes & USER_ENABLED != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_space_of_the_temp_dir_is_positive() {
        assert!(free_space(&std::env::temp_dir()).unwrap() > 0);
    }

    #[test]
    fn free_space_walks_up_to_an_existing_ancestor() {
        let missing = std::env::temp_dir().join("asterium-no-such-dir").join("deeper");
        assert!(free_space(&missing).unwrap() > 0);
    }

    #[test]
    fn x64_runs_on_x64_windows() {
        // An x64 test binary is itself running x64 code, so the answer must be yes wherever the API exists.
        if cfg!(target_arch = "x86_64") {
            let kernel = wide("kernel32.dll");
            let module = unsafe { GetModuleHandleW(kernel.as_ptr()) };
            let has_api = unsafe { GetProcAddress(module, c"GetMachineTypeAttributes".as_ptr().cast()) }.is_some();
            assert_eq!(x64_emulation_available(), has_api);
        }
    }
}
