# OCI Whiteout & Layer Stacking Engine Specification for PodNext v0.2.0

**Author**: Hermes Agent Architecture & Systems Engineering  
**Context**: Porting & Hardening Canonical `proot-distro` Layer Mechanics into Native Rust  
**Target Crate**: `crates/proot-distro-core/src/untar.rs` / `pn-image-store`  
**Status**: Architecture Approved for v0.2.0  

---

## 1. The Multi-Layer OCI Invariant in Rootless Android

A standard OCI container image tracks filesystem modifications across layers with **whiteout markers**:
1. **Opaque Whiteout, marker `.wh..wh..opq`**:
   - Semantics: In an upper layer, this marker signals that lower layers must hide or erase all children of its directory.
   - Example: If `/etc` has `.wh..wh..opq`, the upper layer replaces the entire `/etc` rather than merging with previous layers.
2. **Explicit Entry Whiteout, marker `.wh.<basename>`**:
   - Semantics: Signals that the sibling entry `<basename>` from any lower layer has been deleted.
   - Example: `/bin/.wh.sh` indicates that `/bin/sh` was removed in this layer.

### 1.1 The Attack Vector Dissected from `proot-distro`
A naive slice is dangerous. The pattern is `target = entry_name.strip_prefix(".wh.")`:
- `.wh...` slices to `..` -> invokes `rmtree("parent/..")` -> **Escapes container directory and erases host files!**
- `.wh.` slices to empty string -> references `parent/` -> **Destroys the parent directory.**
- `.wh..` slices to `.` -> references `parent/.` -> **Erases container directory contents.**

---

## 2. The Rust Native Implementation Specification

In `crates/proot-distro-core/src/untar.rs`, the extraction loop must inspect each tar entry header before creation:

```rust
pub enum WhiteoutKind<'a> {
    Opaque,
    Sibling(&'a str),
    None,
}

#[inline]
pub fn classify_whiteout(name: &str) -> WhiteoutKind<'_> {
    if name == ".wh..wh..opq" {
        WhiteoutKind::Opaque
    } else if let Some(target) = name.strip_prefix(".wh.") {
        // Enforce canonical security boundaries: target must be a valid single filename component
        if target.is_empty() || target == "." || target == ".." || target.contains('/') || target.contains('\\') {
            // Malformed or attack payload: absorb marker but do not delete
            WhiteoutKind::None
        } else {
            WhiteoutKind::Sibling(target)
        }
    } else {
        WhiteoutKind::None
    }
}
```

### 2.1 Extraction Flow with Whiteouts
The function is `extract_layer(layer_tar_stream, rootfs_dir)`:
1. Iterate over tar entries.
2. For each entry, split the path into `parent_dir` and `basename`.
3. Call `classify_whiteout(basename)` and check the result:
   - If `WhiteoutKind::Opaque`:
     - Read parent directory entries on disk.
     - Recursively remove all child files and subdirectories within `parent_dir`.
     - **Do not** write the `.wh..wh..opq` marker to disk.
   - If `WhiteoutKind::Sibling(target)`:
     - Form target path: `parent_dir.join(target)`.
     - If the path exists, remove it. Unlink a file or a symlink. Use `remove_dir_all` for a directory.
     - **Do not** write the `.wh.<target>` marker to disk.
   - If `WhiteoutKind::None`:
     - Normal extraction: create directories, write files with a hardlink fallback, and create symlinks.

---

## 3. Atomic Stage & Commit Protocol for v0.2.0

An interrupt during extraction can orphan or corrupt a container. The interrupts are SIGINT, SIGTERM, and the Android LMK. This protocol prevents that result:
1. Extract into staging directory: `containers/<name>.stage.<pid>/`.
2. Apply layers sequentially from 0 to N-1 with whiteout processing.
3. Verify `/bin/sh` or `/bin/bash` exists in the extracted rootfs.
4. Perform atomic rename: `rename("containers/<name>.stage.<pid>", "containers/<name>")`.
5. If extraction fails at any point, unlink the staging directory at once.

This rule guarantees that `podnext` never leaves a partial or unbootable container on disk.
