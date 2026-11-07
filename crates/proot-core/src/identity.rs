//! The identity and credential syscall table (adoption S2).
//!
//! `shroot` decides guest identity with an explicit table of syscalls rather than a single
//! mapping: `native/shroot/src/interception.c:485-491` defines an `IDENTITY(n)` macro and lists
//! the calls that decide identity. Read raw at `shroot` commit `7ab99d6`, that list holds
//! **18** syscalls, `getuid` through `capset`. (PDN's worklist recorded 19; the source says 18,
//! and the rows below are the source's eighteen.) PDN's runtime maps every guest id to one host
//! uid (`--change-id=0:0:0`, `crates/proot-core/src/lib.rs`), so what a guest sees for each of
//! those calls is fixed - but until now it was named nowhere: it was implicit in the runtime's
//! argv, visible only to a reader who already knew where to look.
//!
//! This module is that named place. One row per syscall that can change or report a guest's
//! identity, each with what PDN does about it. The count is written down once, as
//! `IDENTITY_SYSCALL_COUNT`, and pinned to the rows by a compile-time assertion and by
//! `tests/identity_table.rs`, so the table and its own count cannot drift apart - the defect this
//! project keeps finding in tables whose comment disagrees with their contents.
//!
//! The split is by the call's stated function: `getuid`, `getgroups`, `capget` and their
//! siblings *report* identity, so a guest is answered with the configured identity; `setuid`,
//! `setgroups`, `capset` and their siblings *change* it, and PDN adds no policy for them - the
//! runtime takes them, and per the rule carried from `shroot` (`docs/shroot.md`: a denied
//! operation never switches identity) a change that does not take effect never re-identifies the
//! process. `setfsuid` and `setfsgid` also return the previous value; they are grouped with the
//! changes because changing the filesystem identity is what the call is for.

/// What PDN does about one identity or credential syscall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityDisposition {
    /// The guest is answered with the configured identity (the mapped root), never with the
    /// executor's real value.
    ReportGuest,
    /// PDN installs no policy: the call is left to the runtime, which translates it against the
    /// configured identity, so it never switches the executor's real uid.
    PassThrough,
}

use IdentityDisposition::{PassThrough, ReportGuest};

/// The syscalls that can change or report a guest's identity, in `shroot`'s order
/// (`native/shroot/src/interception.c:487-491`), each with what PDN does about it.
///
/// This is the one place a caller has to read to see which calls are handled: a call absent from
/// this table is one PDN names no disposition for, which `identity_disposition` reports as `None`
/// rather than inventing an answer.
pub const IDENTITY_SYSCALL_TABLE: &[(&str, IdentityDisposition)] = &[
    // Report identity: the guest is told the configured identity.
    ("getuid", ReportGuest),
    ("geteuid", ReportGuest),
    ("getgid", ReportGuest),
    ("getegid", ReportGuest),
    ("getresuid", ReportGuest),
    ("getresgid", ReportGuest),
    // Change identity: the runtime takes the call and never switches the executor's real uid.
    ("setresuid", PassThrough),
    ("setresgid", PassThrough),
    ("setuid", PassThrough),
    ("setgid", PassThrough),
    ("setreuid", PassThrough),
    ("setregid", PassThrough),
    ("setfsuid", PassThrough),
    ("setfsgid", PassThrough),
    ("setgroups", PassThrough),
    ("getgroups", ReportGuest),
    ("capget", ReportGuest),
    ("capset", PassThrough),
];

/// The number of rows in [`IDENTITY_SYSCALL_TABLE`], written down once so the table, this
/// documentation and `tests/identity_table.rs` all cite the same number: **18**.
pub const IDENTITY_SYSCALL_COUNT: usize = 18;

// The rows and the documented count are the same number or the crate does not build - a table
// whose own comment disagrees with its contents cannot be committed, let alone documented.
const _: () = assert!(IDENTITY_SYSCALL_TABLE.len() == IDENTITY_SYSCALL_COUNT);

/// What PDN does about `name`, or `None` when it is not one of the syscalls PDN names as an
/// identity or credential call. `None` is the honest answer for a call outside the table: it is
/// not silently treated as handled.
pub fn identity_disposition(name: &str) -> Option<IdentityDisposition> {
    IDENTITY_SYSCALL_TABLE
        .iter()
        .find(|(syscall, _)| *syscall == name)
        .map(|(_, disposition)| *disposition)
}
