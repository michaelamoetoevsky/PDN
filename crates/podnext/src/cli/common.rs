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
/// `Proot` (the default) spawns the host `proot` binary - today's behaviour, byte for byte.
/// `Native` runs the guest under podnext's own in-tree ptrace engine
/// (`proot_core::native_run`), touching no `proot` binary at all. The choice is opt-in: the
/// default is `Proot` so nothing about the existing path changes.
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
