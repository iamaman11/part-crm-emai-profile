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


const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 0x0002;

pub(super) fn copy_redacted_report(report: &str) -> bool {
    let utf16: Vec<u16> = report.encode_utf16().chain([0]).collect();
    let Some(size) = utf16.len().checked_mul(core::mem::size_of::<u16>()) else {
        return false;
    };
    // SAFETY: The same thread owns the clipboard for this bounded synchronous operation.
    // GlobalAlloc is freed on every failure before ownership transfers to SetClipboardData.
    // No secret/raw user bytes can enter: the caller supplies a typed redacted report.
    unsafe {
        // OpenClipboard(NULL) followed by EmptyClipboard() leaves no clipboard
        // owner, so SetClipboardData can fail. Own a private, never shown
        // system-class window for this one synchronous copy operation.
        let window_class: Vec<u16> = "STATIC".encode_utf16().chain([0]).collect();
        let owner = CreateWindowExW(
            0, window_class.as_ptr(), ptr::null(), 0,
            0, 0, 0, 0, ptr::null_mut(), ptr::null_mut(),
            ptr::null_mut(), ptr::null(),
        );
        if owner.is_null() {
            return false;
        }
        if OpenClipboard(owner) == 0 {
            let _ = DestroyWindow(owner);
            return false;
        }
        let success = if EmptyClipboard() == 0 {
            false
        } else {
            let handle = GlobalAlloc(GMEM_MOVEABLE, size);
            if handle.is_null() {
                false
            } else {
                let destination = GlobalLock(handle);
                if destination.is_null() {
                    let _ = GlobalFree(handle);
                    false
                } else {
                    core::ptr::copy_nonoverlapping(
                        utf16.as_ptr(),
                        destination.cast::<u16>(),
                        utf16.len(),
                    );
                    let _ = GlobalUnlock(handle);
                    if SetClipboardData(CF_UNICODETEXT, handle).is_null() {
                        let _ = GlobalFree(handle);
                        false
                    } else {
                        // The clipboard now owns the movable allocation.
                        true
                    }
                }
            }
        };
        let _ = CloseClipboard();
        let _ = DestroyWindow(owner);
        success
    }
}

pub(super) fn show_report_copied() {
    let message: Vec<u16> = "Profile Bridge: диагностика скопирована в буфер обмена."
        .encode_utf16()
        .chain([0])
        .collect();
    let title: Vec<u16> = "Profile Bridge".encode_utf16().chain([0]).collect();
    // SAFETY: Static bounded UTF-16 strings remain valid during the synchronous dialog.
    unsafe {
        let _ = MessageBoxW(ptr::null_mut(), message.as_ptr(), title.as_ptr(), 0x40);
    }
}

#[link(name = "user32")]
unsafe extern "system" {
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32, y: i32, width: i32, height: i32,
        parent: *mut c_void, menu: *mut c_void,
        instance: *mut c_void, parameter: *const c_void,
    ) -> *mut c_void;
    fn DestroyWindow(window: *mut c_void) -> i32;
    fn OpenClipboard(owner: *mut c_void) -> i32;
    fn EmptyClipboard() -> i32;
    fn CloseClipboard() -> i32;
    fn SetClipboardData(format: u32, data: *mut c_void) -> *mut c_void;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void;
    fn GlobalLock(handle: *mut c_void) -> *mut c_void;
    fn GlobalUnlock(handle: *mut c_void) -> i32;
    fn GlobalFree(handle: *mut c_void) -> *mut c_void;
}
