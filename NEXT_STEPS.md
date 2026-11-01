# PDN Next Steps & Roadmap (v0.1.7 — 2026-11-01)

**Workspace**: `/data/data/com.termux/files/home/workspace/PDN`
**Repository**: `https://github.com/michaelamoetoevsky/PDN`
**Active Version**: `0.1.7` (Centralized across all 6 crates in `Cargo.toml`)
**Git HEAD**: `9e778740d41738a37a98543c20ae2b22d207cda4` (branch `master`)

---

## 1. Status Resolusi Sesi Terkini (2026-11-01)

Sesi ini berfokus pada audit empiris mendalam, eliminasi klaim usang, penyelarasan versi workspace, pembersihan debug leakage, standardisasi format/whitespace, dan verifikasi runtime daemon baseline:

- ✅ **Penyelarasan Metadata & Versi**: Root `Cargo.toml` kini memusatkan `[workspace.package]` versi `0.1.7`, edition 2021, lisensi, dan URL repositori resmi `https://github.com/michaelamoetoevsky/PDN`. Keenam crate (`proot-core`, `proot-distro-core`, `proot-compat`, `pn-gateway`, `pn-next`, `podnext`) mewarisi versi `0.1.7`.
- ✅ **Eliminasi Debug Leakage & Versioning Dinamis**:
  - `crates/podnext/src/main.rs`: Flag CLI biner diubah dari hardcoded `0.1.0` ke `#[command(version)]`. Debug log `eprintln!("[DEBUG main]...")` dikunci di balik variabel lingkungan `PN_NEXT_DEBUG`.
  - `crates/podnext/src/daemon.rs`: Flag daemon diubah dari hardcoded `0.1.0` ke `#[command(version)]`.
- ✅ **Pembersihan Root Scratch & Git Hygiene**:
  - Berkas probe/ELF scratch dibersihkan dari root workspace.
  - `.gitignore` diperbarui untuk mengecualikan pola root scratch.
  - Direktori duplikat `test_supervisor` (606 MB) dihapus.
- ✅ **Standardisasi Format & Whitespace**:
  - Eksekusi `cargo fmt --all` tuntas: `FMT_CHECK_EXIT=0` (0 diff lines).
  - Pemeriksaan `git diff --check` tuntas: `DIFFCHECK_EXIT=0` (0 trailing whitespace).
- ✅ **Penyelarasan Resep Termux Package**:
  - `termux-package/recipe/build.sh` diperbarui ke `TERMUX_PKG_VERSION="0.1.7"`.
- ✅ **Verifikasi Build & Pengujian Empiris**:
  - `cargo build --package podnext --release`: Lolos, biner `target/release/podnext` (15,3 MB) dan `target/release/podnext-daemon` (12,3 MB) terverifikasi ELF 64-bit aarch64 Android 24.
  - `cargo test --workspace`: 93 test lolos, 1 ignored, 0 gagal (`pdn-cargo-test-postpatch-20261101-181037.log`).
  - `cargo test --package pn-next --features cri`: 49 test lolos, 0 gagal (`pdn-pn-next-cri-test-20261101.log`).
  - Daemon IPC Smoke Test: Socket Unix merespons `ping` -> `{"type":"pong"}` dan `list` -> `{"type":"ok","data":[]}`.

---

## 2. Agenda & Roadmap Pengembangan Lanjutan

Pekerjaan berikutnya dibagi ke dalam prioritas terstruktur berdasarkan hasil audit:

### Prioritas P0: Pengujian Integrasi Nyata & Gating Harness (Critical)
1. **Daemon Client Integration Test Suite**:
   - `crates/podnext/examples/test_daemon_client.rs` saat ini hanya berupa compile check (`0 tests`).
   - Buat integration test terdaftar di `tests/daemon_integration_test.rs` yang menguji siklus hidup daemon melalui soket Unix nyata (start, subscribe events, register service, status, stop).
2. **Koneksi `tests/integration/v2_criteria.rs`**:
   - Daftarkan target pengujian kriteria v2 ke dalam Cargo harness agar otomatis dieksekusi saat `cargo test --workspace`.
3. **Pembersihan Compiler Warnings**:
   - Tangani dead code warning (`parse_container_path`, `dir_size`, `parse_size`, `ImageLayer`).
   - Selesaikan unused variables/mutability di `crates/podnext/src/daemon.rs` (`_config`, `_supervisor`, `_events`).
   - Pantau laporan kompatibilitas `nom v6.1.2`.

### Prioritas P1: Implementasi Stub Subcommand CLI (High)
1. **Subcommand Image**:
   - Selesaikan implementasi `podnext image push`, `tag`, `history`, dan `inspect` yang saat ini berstatus stub (`unimplemented!()` / TODO).
2. **Subcommand Network**:
   - Hubungkan handler `podnext network create|list|inspect|rm` dengan driver CNI / bridge di `proot-distro-core` / `pn-next`.
3. **Subcommand Volume**:
   - Hubungkan handler `podnext volume create|list|inspect|rm` dengan driver penyimpanan lokal di `proot-distro-core`.

### Prioritas P2: Pematangan Runtime & Supervisi Daemon (Medium)
1. **Service Supervisor E2E**:
   - Uji persistensi file servis TOML di `$PREFIX/var/service/pdn/` atau `~/.config/podnext/services/`.
   - Implementasi restart policy (always, on-failure, unless-stopped) dengan guardrail circuit breaker.
2. **Termux:Boot Integration**:
   - Validasi script boot di `$HOME/.termux/boot/` untuk menjalankan daemon saat perangkat menyala.
3. **D-Bus Broker / IPC Expansion**:
   - Evaluasi penggunaan soket Unix vs D-Bus murni untuk komunikasi inter-container di lingkungan Android non-root.

### Prioritas P3: CI/CD & Ekosistem (Future)
1. **Android Cross-Compilation di GitHub Actions**:
   - Aktifkan CI matrix untuk `aarch64-linux-android` dan `x86_64-linux-android` menggunakan NDK r27c/r30.
2. **Publikasi Rilis GitHub v0.1.7**:
   - Setelah commit lokal disetujui dan di-push, buat release GitHub resmi `v0.1.7` lengkap dengan biner rilis dan checksum SHA-256 (mengingat rilis remote publik saat ini baru sampai `v0.1.5`).
3. **Follow-up PR termux-packages**:
   - Pantau review PR `#33069` di repositori termux/termux-packages dan perbarui resep jika diperlukan.

---

## 3. Blok Perintah Verifikasi Cepat (Siap Tempel di Termux)

```sh
cd ~/workspace/PDN

# 1. Cek kepatuhan format & whitespace
env -u LD_LIBRARY_PATH cargo fmt --all -- --check
git diff --check

# 2. Verifikasi versi biner rilis
./target/release/podnext --version
./target/release/podnext-daemon --version

# 3. Jalankan suite pengujian workspace & CRI
env -u LD_LIBRARY_PATH cargo test --workspace
env -u LD_LIBRARY_PATH cargo test --package pn-next --features cri

# 4. Monitor proses aktif (pastikan tidak ada compiler/daemon menggantung)
ps -A -o pid=,stat=,args= | grep -E '[c]argo|[r]ustc|[p]odnext' || true
```
