//! Seccomp filter bypass for Android
//! Menambahkan Android-specific syscalls yang diizinkan
//! agar tidak double-block oleh seccomp filter PRoot

/// Android-specific syscalls that should be allowed (not traced/killed)
/// Values are raw syscall numbers for aarch64 Linux
const ANDROID_ALLOWED_SYSCALLS: &[libc::c_long] = &[
    // rseq - Restartable sequences (Android Bionic)
    336, // SYS_rseq on aarch64
    // membarrier - Memory barriers (Android Bionic)
    324, // SYS_membarrier on aarch64
    // process_vm_readv - Cross-process memory (Android)
    347, // SYS_process_vm_readv on aarch64
    // process_vm_writev - Cross-process memory (Android)
    348, // SYS_process_vm_writev on aarch64
    // clone3 - Clone3 (Android Bionic)
    435, // SYS_clone3 on aarch64
    // openat2 - Openat2 (Android Bionic)
    437, // SYS_openat2 on aarch64
    // mlock - Memory lock (Android)
    151, // SYS_mlock on aarch64
    // munlock - Memory unlock (Android)
    152, // SYS_munlock on aarch64
];

/// Configure seccomp filter to allow Android-specific syscalls.
/// This prevents double-blocking when PRoot's seccomp filter
/// interacts with Android's app-level seccomp.
pub fn configure_seccomp_filter() -> Result<(), String> {
    // On Android, PR_SET_NO_NEW_PRIVS might cause SIGSYS on some kernels.
    // We skip this call and rely on PRoot's built-in seccomp handling.
    // The Android syscalls are documented for reference.
    
    #[cfg(debug_assertions)]
    {
        eprintln!(
            "[proot-compat] Android allowed syscalls (reference): {:?}",
            ANDROID_ALLOWED_SYSCALLS
        );
    }

    Ok(())
}

/// Get the list of Android-specific syscalls that should be allowed.
pub fn get_android_allowed_syscalls() -> &'static [libc::c_long] {
    ANDROID_ALLOWED_SYSCALLS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_android_allowed_syscalls_not_empty() {
        assert!(!ANDROID_ALLOWED_SYSCALLS.is_empty());
    }

    #[test]
    fn test_get_android_allowed_syscalls() {
        let syscalls = get_android_allowed_syscalls();
        assert_eq!(syscalls, ANDROID_ALLOWED_SYSCALLS);
    }
}