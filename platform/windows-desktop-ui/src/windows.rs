use core::ffi::c_void;
use std::ptr;

const STD_ERROR_HANDLE: u32 = (-12_i32) as u32;
const MB_OK_ICONERROR: u32 = 0x10;
const MAX_MESSAGE_BYTES: usize = 4_096;

pub(super) fn show_error_unless_redirected(title: &str, message: &str) -> bool {
    if title.is_empty()
        || title.len() > 128
        || message.is_empty()
        || message.len() > MAX_MESSAGE_BYTES
        || title.contains('\0')
        || message.contains('\0')
    {
        return false;
    }
    // SAFETY: GetStdHandle returns a borrowed process handle; GetFileType does not take ownership
    // and accepts a missing/invalid handle by returning FILE_TYPE_UNKNOWN. Never close this handle.
    let file_type = unsafe { GetFileType(GetStdHandle(STD_ERROR_HANDLE)) };
    if super::redirected_file_type(file_type) {
        return false;
    }
    let title: Vec<u16> = title.encode_utf16().chain([0]).collect();
    let message: Vec<u16> = message.encode_utf16().chain([0]).collect();
    // SAFETY: both bounded UTF-16 buffers are NUL terminated and live for the synchronous call.
    // NULL means no owner window. No system-modal/topmost/service notification flags are used.
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK_ICONERROR,
        ) != 0
    }
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(kind: u32) -> *mut c_void;
    fn GetFileType(handle: *mut c_void) -> u32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(owner: *mut c_void, text: *const u16, title: *const u16, flags: u32) -> i32;
}
