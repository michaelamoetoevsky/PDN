//! Digest validation before path composition (WORKLIST B1).
//!
//! Comparator: proot-distro `docker/cache.py:validate_digest`, described in NEXT_DEV.md §5.1 as
//! "a choke point before any conversion of an untrusted digest into a filesystem path"; their
//! `_oci_blob_path` validates first so "a crafted index.json cannot route the lookup through a
//! member name".
//!
//! Nothing here does I/O. `image.rs` validates at the one place it composes a request path out of
//! a digest, before the URL exists; the digest also reaches the digest-mismatch check downstream,
//! where a validated value is the only kind that means what it says.

use anyhow::{bail, Result};

/// The digest algorithm this client produces and consumes. OCI permits others; none reaches PDN.
const ALGORITHM: &str = "sha256";

/// Number of hex characters in a sha256 digest.
const HEX_LEN: usize = 64;

/// Validate a content digest *before* anything composes a path, URL or slice from it (B1).
///
/// The accepted form is exactly `<algorithm>:<hex>` with a supported algorithm and 64 lowercase
/// hex characters. Everything else is refused, and the refusal quotes the offending value: a
/// digest carrying `..` or a path separator is the shape that turns an untrusted manifest field
/// into a path or URL that points somewhere else, so the value that did it belongs in the message.
pub fn validate_digest(digest: &str) -> Result<()> {
    let well_formed = digest
        .strip_prefix(ALGORITHM)
        .and_then(|rest| rest.strip_prefix(':'))
        .is_some_and(|encoded| {
            encoded.len() == HEX_LEN
                && encoded
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        });

    if !well_formed {
        bail!("refusing digest {digest:?}: expected `{ALGORITHM}:<{HEX_LEN} lowercase hex>`");
    }
    Ok(())
}
