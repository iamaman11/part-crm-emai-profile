#![deny(unsafe_code)]

//! Narrow Windows presentation adapter. No process, network, key or enrollment authority.

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

/// Keep an interactive launch error visible until the user dismisses it.
///
/// Callers must render the same diagnosis to stderr first. A redirected file/pipe is a deliberate
/// CLI/test capture destination; it must never be blocked by a modal desktop window.
pub fn show_error_unless_redirected(
    title: &'static str,
    code: &'static str,
    message: &'static str,
) -> bool {
    #[cfg(windows)]
    {
        windows::show_error_unless_redirected(title, &format!("{message}\n\nCode: {code}"))
    }
    #[cfg(not(windows))]
    {
        let _ = (title, code, message);
        false
    }
}

#[cfg(any(windows, test))]
fn redirected_file_type(file_type: u32) -> bool {
    // FILE_TYPE_REMOTE is an obsolete modifier, not a distinct destination type.
    matches!(file_type & !0x8000, 1 | 3)
}

#[cfg(test)]
mod tests {
    use super::redirected_file_type;

    #[test]
    fn captured_errors_never_require_an_interactive_desktop() {
        assert!(redirected_file_type(1)); // FILE_TYPE_DISK
        assert!(redirected_file_type(3)); // FILE_TYPE_PIPE
        assert!(redirected_file_type(0x8003));
        assert!(!redirected_file_type(2)); // FILE_TYPE_CHAR / console
        assert!(!redirected_file_type(0)); // no attached console
    }
}
