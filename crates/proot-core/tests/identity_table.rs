//! Adoption S2: the identity/credential syscall table must be countable, complete and honest.
//!
//! `shroot` lists the syscalls that decide guest identity in one table
//! (`native/shroot/src/interception.c:485-491`). PDN's runtime maps every guest id to one host uid
//! and named that set nowhere; `proot_core::identity` is the named place, and this test is what
//! keeps its documentation and its rows from disagreeing. It counts the rows against the
//! documented number, checks the rows against an independent statement of them (a table compared
//! against a count read from itself proves nothing), and asserts that a syscall *outside* the
//! table is not silently claimed to be handled.

use proot_core::identity::{
    identity_disposition, IdentityDisposition, IDENTITY_SYSCALL_COUNT, IDENTITY_SYSCALL_TABLE,
};

/// The eighteen syscalls `shroot` lists, in its order, with PDN's disposition. Retyped on
/// purpose: the table is compared against this statement, not against a count derived from it.
const EXPECTED: [(&str, IdentityDisposition); 18] = [
    ("getuid", IdentityDisposition::ReportGuest),
    ("geteuid", IdentityDisposition::ReportGuest),
    ("getgid", IdentityDisposition::ReportGuest),
    ("getegid", IdentityDisposition::ReportGuest),
    ("getresuid", IdentityDisposition::ReportGuest),
    ("getresgid", IdentityDisposition::ReportGuest),
    ("setresuid", IdentityDisposition::PassThrough),
    ("setresgid", IdentityDisposition::PassThrough),
    ("setuid", IdentityDisposition::PassThrough),
    ("setgid", IdentityDisposition::PassThrough),
    ("setreuid", IdentityDisposition::PassThrough),
    ("setregid", IdentityDisposition::PassThrough),
    ("setfsuid", IdentityDisposition::PassThrough),
    ("setfsgid", IdentityDisposition::PassThrough),
    ("setgroups", IdentityDisposition::PassThrough),
    ("getgroups", IdentityDisposition::ReportGuest),
    ("capget", IdentityDisposition::ReportGuest),
    ("capset", IdentityDisposition::PassThrough),
];

#[test]
fn the_table_holds_the_number_its_documentation_states() {
    assert_eq!(
        IDENTITY_SYSCALL_COUNT, 18,
        "the documented count is 18 (the source says 18, not the 19 the worklist records)"
    );
    assert_eq!(
        IDENTITY_SYSCALL_TABLE.len(),
        IDENTITY_SYSCALL_COUNT,
        "the table has {} rows, but its documentation counts {IDENTITY_SYSCALL_COUNT}",
        IDENTITY_SYSCALL_TABLE.len()
    );
}

#[test]
fn the_rows_are_exactly_the_syscalls_shroot_lists() {
    assert_eq!(
        IDENTITY_SYSCALL_TABLE,
        &EXPECTED[..],
        "the named table must hold these syscalls with these dispositions"
    );
}

#[test]
fn no_syscall_is_named_twice() {
    let mut names: Vec<&str> = IDENTITY_SYSCALL_TABLE.iter().map(|(n, _)| *n).collect();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        total,
        "a syscall named twice would make the count a lie"
    );
}

#[test]
fn every_row_resolves_and_nothing_outside_the_table_is_claimed() {
    // The lookup and the table cannot disagree.
    for (name, disposition) in IDENTITY_SYSCALL_TABLE {
        assert_eq!(
            identity_disposition(name),
            Some(*disposition),
            "the table names '{name}', so the lookup must return its disposition"
        );
    }

    // A syscall OUTSIDE the table must answer `None`. These are the near misses: real,
    // identity-adjacent calls that `shroot`'s table does not list, including the 32-bit spellings
    // PDN's runtime does not name. Claiming a disposition for any of them would make the table
    // broader than its source and silent about the mismatch.
    for absent in [
        "prctl", "clone3", "unshare", "setns", "getpid", "getppid", "setuid32", "getuid32",
    ] {
        assert_eq!(
            identity_disposition(absent),
            None,
            "'{absent}' is not in the table, so PDN must not claim to handle it"
        );
    }
}
