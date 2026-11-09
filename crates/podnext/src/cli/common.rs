//! Shared value enums for the CLI surface.
//!
//! The output format plus the option families more than one sub-command accepts
//! (`--restart-policy`, `--resource`, `--min-level`, the wake-lock type, the
//! backup type, the completion shell). They live together because no single
//! domain module owns them.

use clap::ValueEnum;

/// Backup type
#[derive(ValueEnum, Clone, Copy, Debug)]
#[clap(rename_all = "lowercase")]
pub(crate) enum BackupTypeArg {
    Full,
    Incremental,
    Differential,
    ConfigOnly,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    Table,
    Json,
    Yaml,
    Text,
}

/// Which engine runs a guest for `run`/`login`.
///
/// `Native` (the DEFAULT since 2026-11-09) runs the guest under podnext's own in-tree ptrace engine
/// (`proot_core::native_run`) and touches no `proot` binary at all. It became the default only after
/// the parity and adequacy work landed and was measured: the harness `scripts/engine_differential.py`
/// reports EQUAL on every field it compares, and a real Debian 13 rootfs runs ordinary shell
/// workloads (`ls`, `apt-get`, `dpkg`, pipelines) with output identical to the proot arm.
/// `Proot` remains available (`--engine proot`) and still spawns the host `proot` binary, byte for
/// byte, so the previous behaviour is one flag away rather than gone.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "lowercase")]
pub(crate) enum EngineChoice {
    Proot,
    Native,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub(crate) enum RestartPolicyArg {
    Never,
    OnFailure,
    Always,
    UnlessStopped,
    OnFailureOrCrash,
}

#[derive(ValueEnum, Clone, Copy, Debug, strum::Display)]
#[strum(serialize_all = "lowercase")]
pub(crate) enum ResourceTypeArg {
    Cpu,
    Memory,
    MemorySwap,
    IoReadBps,
    IoWriteBps,
    IoReadIops,
    IoWriteIops,
    Pids,
    Nofile,
    Network,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub(crate) enum LogLevelArg {
    Emergency,
    Alert,
    Critical,
    Error,
    Warning,
    Notice,
    Info,
    Debug,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub(crate) enum WakeLockTypeArg {
    Partial,
    Full,
    DozeExemption,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub(crate) enum Shell {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Elvish,
}
