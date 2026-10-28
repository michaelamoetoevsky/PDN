//! Compatibility layer for PRoot

pub mod path_translation {
    use std::path::{Path, PathBuf};
    use anyhow::Result;

    /// Resolve symlink chain with a maximum depth to prevent infinite loops
    pub fn resolve_symlink_chain(path: &Path) -> Result<PathBuf> {
        let mut current = path.to_path_buf();
        let mut depth = 0;
        const MAX_DEPTH: usize = 40;

        while depth < MAX_DEPTH {
            if !current.exists() {
                break;
            }

            match std::fs::read_link(&current) {
                Ok(target) => {
                    if target.is_absolute() {
                        current = target;
                    } else {
                        current = current.parent().unwrap_or(&PathBuf::from("/")).join(target);
                    }
                    depth += 1;
                }
                Err(_) => break,
            }
        }

        if depth >= MAX_DEPTH {
            return Err(anyhow::anyhow!("Symlink chain too deep (max {})", MAX_DEPTH));
        }

        Ok(current)
    }
}

pub mod platform {
    /// Platform-specific personality/spoofing
    pub fn get_personality() -> &'static str {
        "Linux"
    }
}

pub mod seccomp_bypass {
    /// Seccomp bypass for Android syscalls
    pub const ALLOWED_ANDROID_SYSCALLS: &[i64] = &[
        336,  // rseq
        324,  // membarrier
        347,  // process_vm_readv
        348,  // process_vm_writev
        435,  // clone3
        437,  // openat2
        151,  // mlock
        152,  // munlock
        439,  // faccessat2
        291,  // statx
        301,  // io_pgetevents
        424,  // pidfd_send_signal
        434,  // pidfd_open
        438,  // pidfd_getfd
        440,  // process_madvise
        441,  // epoll_pwait2
        442,  // mount_setattr
        443,  // quotactl_fd
        444,  // landlock_create_ruleset
        445,  // landlock_add_rule
        446,  // landlock_restrict_self
        447,  // memfd_secret
        448,  // process_mrelease
        449,  // futex_waitv
        450,  // set_mempolicy_home_node
        451,  // cachedctl
        452,  // sched_setattr
        453,  // move_mount
        454,  // fsopen
        455,  // fsconfig
        456,  // fsmount
        457,  // fspick
        458,  // move_pages
    ];
}