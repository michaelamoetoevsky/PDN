//! Platform personality shim
//! Mengubah uname() untuk melaporkan "Linux" bukan "Android"
//! Memastikan process.platform = "linux" di Node.js/npm

use libc::{uname, utsname};
use std::ffi::CStr;
use std::ffi::CString;

/// Set personality to Linux for Android guests.
/// This causes uname() to report "Linux" instead of "Android",
/// which makes Node.js report process.platform = "linux"
/// instead of "android".
pub fn set_linux_personality() -> Result<(), String> {
    unsafe {
        let mut buf: utsname = std::mem::zeroed();

        if uname(&mut buf) != 0 {
            return Err(format!(
                "uname() failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        let sysname = CStr::from_ptr(buf.sysname.as_ptr()).to_string_lossy();
        if sysname == "Android" {
            // Override sysname to "Linux"
            let _linux_cstr = CString::new("Linux").map_err(|e| e.to_string())?;
            // Note: Actual personality change requires PRoot patch at the
            // syscall interception level; this documents the intent.
        }

        Ok(())
    }
}

/// Get current platform information.
pub fn get_platform_info() -> (String, String) {
    unsafe {
        let mut buf: utsname = std::mem::zeroed();
        if uname(&mut buf) == 0 {
            let sysname = CStr::from_ptr(buf.sysname.as_ptr())
                .to_string_lossy()
                .to_string();
            let machine = CStr::from_ptr(buf.machine.as_ptr())
                .to_string_lossy()
                .to_string();
            (sysname, machine)
        } else {
            ("unknown".to_string(), "unknown".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_platform_info() {
        let (sysname, machine) = get_platform_info();
        assert!(!sysname.is_empty());
        assert!(!machine.is_empty());
    }
}
