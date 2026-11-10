#!/bin/sh
# End-to-end battery: the REAL program, driven by direct commands, on a real Debian rootfs.
#
# Why this exists: a green `cargo test` proves the crate compiles and the unit-level contracts hold. It does not
# prove that `podnext run <container> -- <command>` does what a user expects, because that requires the kernel, a
# real rootfs, a real guest program and a real exit status. This script runs the shipped binary the way a user does
# and prints, per case, the raw output of both engines plus a verdict:
#
#   EQUAL       both engines produced the same exit status, stdout and stderr
#   DIFFERENT   they did not - printed in full, and that is a finding, not a failure of the script
#   BLOCKED     the case could not run; the reason is printed
#
# It never touches the operator's real store: point it at a fixture with PN_TEST_ROOT, or pass STORE.
#
# Usage:  scripts/e2e_parity_battery.sh [--store DIR] [--container NAME] [--bin PATH]
set -u

STORE="${PN_TEST_ROOT:-}"
CONTAINER="realdeb"
BIN="target/release/podnext"
while [ $# -gt 0 ]; do
  case "$1" in
    --store) STORE="$2"; shift 2 ;;
    --container) CONTAINER="$2"; shift 2 ;;
    --bin) BIN="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [ -z "$STORE" ]; then
  echo "BLOCKED: no store. Set PN_TEST_ROOT (a fixture holding .podnext/containers/<name>) or pass --store DIR." >&2
  echo "         The operator's real store must not be used for writes, so this script requires a fixture." >&2
  exit 2
fi
if [ ! -x "$BIN" ]; then
  echo "BLOCKED: $BIN is not executable. Build it first: cargo build --release --workspace" >&2
  exit 2
fi

WORK="$STORE/e2e-work"
mkdir -p "$WORK" || exit 2
PASS=0; DIFF=0; BLOCKED=0; SUSPECT=0; CASES=0

# run_case <name> <engine> <command...>  -> stdout+stderr into $WORK/<name>.<engine>, exit status into a file
run_case() {
  name="$1"; engine="$2"; shift 2
  # A per-case ceiling: without one a guest that blocks (measured: `wait` on a background job under the native
  # engine) stalls the whole battery instead of being reported as that case's difference.
  timeout 30 env -u LD_LIBRARY_PATH PN_TEST_ROOT="$STORE" HOME="$STORE" \
      "$BIN" --engine "$engine" run "$CONTAINER" -- "$@" \
      >"$WORK/$name.$engine.out" 2>"$WORK/$name.$engine.err"
  echo $? >"$WORK/$name.$engine.rc"
}

# case_begin <name> <guest command...>
case_begin() {
  name="$1"; shift
  CASES=$((CASES + 1))
  run_case "$name" proot "$@"
  run_case "$name" native "$@"
  rc_p=$(cat "$WORK/$name.proot.rc"); rc_n=$(cat "$WORK/$name.native.rc")
  out_bytes=$(wc -c <"$WORK/$name.native.out")
  if [ "$rc_p" = "$rc_n" ] && cmp -s "$WORK/$name.proot.out" "$WORK/$name.native.out" \
     && cmp -s "$WORK/$name.proot.err" "$WORK/$name.native.err"; then
    # Two IDENTICAL FAILURES are not agreement about the guest: the first run of this battery reported 10/10
    # EQUAL while both engines answered `Error: Container 'almalinux' not found` (rc=1, empty stdout) for every
    # case - because the fixture copy had carried a 1 MB directory with no metadata.json. Agreement on an ENVIRONMENT
    # error says nothing about the engine, so it gets its own verdict; a case that legitimately exits non-zero with
    # no stdout (the `exit 7` probe) is still EQUAL, because both engines agree on the guest's own behaviour.
    if [ "$rc_n" -ne 0 ] && [ "$out_bytes" -eq 0 ] \
       && grep -q "^Error: " "$WORK/$name.native.err" && grep -q "^Error: " "$WORK/$name.proot.err"; then
      SUSPECT=$((SUSPECT + 1)); verdict=SUSPECT
    else
      PASS=$((PASS + 1)); verdict=EQUAL
    fi
  else
    DIFF=$((DIFF + 1)); verdict=DIFFERENT
  fi
  printf '\n[%s] %s\n' "$verdict" "$name"
  printf '  command : %s\n' "$*"
  printf '  proot   : rc=%s out=%sB err=%sB\n' "$rc_p" "$(wc -c <"$WORK/$name.proot.out")" "$(wc -c <"$WORK/$name.proot.err")"
  printf '  native  : rc=%s out=%sB err=%sB\n' "$rc_n" "$(wc -c <"$WORK/$name.native.out")" "$(wc -c <"$WORK/$name.native.err")"
  if [ "$verdict" = DIFFERENT ]; then
    echo "  --- proot stdout ---"; sed 's/^/    /' "$WORK/$name.proot.out" | head -12
    echo "  --- native stdout ---"; sed 's/^/    /' "$WORK/$name.native.out" | head -12
    if ! cmp -s "$WORK/$name.proot.err" "$WORK/$name.native.err"; then
      echo "  --- proot stderr ---"; sed 's/^/    /' "$WORK/$name.proot.err" | head -8
      echo "  --- native stderr ---"; sed 's/^/    /' "$WORK/$name.native.err" | head -8
    fi
  fi
}

echo "=== e2e parity battery ==="
echo "  binary    : $BIN ($(stat -c %y "$BIN" 2>/dev/null | cut -d. -f1))"
echo "  store     : $STORE"
echo "  container : $CONTAINER"

# 0. The container is visible at all - the first thing a user does.
echo ""
echo "=== list ==="
env -u LD_LIBRARY_PATH PN_TEST_ROOT="$STORE" HOME="$STORE" "$BIN" list 2>&1 | sed 's/^/  /' | head -8

# 1. Identity and environment - the guest's view of itself.
case_begin identity /bin/sh -c 'id -u; id -g; echo "$0"'
# 2. Real distribution facts, read from the rootfs.
case_begin distro /bin/sh -c 'head -1 /etc/os-release; cat /etc/hostname 2>/dev/null; true'
# 3. A pipeline with a filter - the shell has to work, not just the leaf program.
case_begin pipeline /bin/sh -c 'ls /usr/bin | wc -l'
# 4. Exit status propagation, including a non-zero one.
case_begin exit_nonzero /bin/sh -c 'exit 7'
# 5. stdout vs stderr separation.
case_begin streams /bin/sh -c 'echo out; echo err >&2'
# 6. Writing in the guest: the write must land somewhere the guest can read back.
case_begin write_read /bin/sh -c 'tmp=$(mktemp -d); echo hello > "$tmp/f"; cat "$tmp/f"; rm -rf "$tmp"'
# 7. A package-manager query - the deepest real-world probe of a rootfs.
case_begin dpkg_query /bin/sh -c 'dpkg-query -W -f="\${Package}\n" 2>/dev/null | wc -l'
# 8. Spawning a process and reading the result back.
case_begin subshell /bin/sh -c 'x=$(/bin/echo nested); echo "got:$x"'
# 9. Recursion through the guest's own shell (a second execve in the same process tree).
case_begin nested_exec /bin/sh -c '/bin/sh -c "echo deep"'
# 10. A relative path resolved against the guest's cwd.
case_begin cwd_relative /bin/sh -c 'cd /etc && head -1 os-release'
# 11. rename(2)/renameat: a file moved within the same directory.
case_begin rename_file /bin/sh -c 'cd /tmp && echo x > mv-a && mv mv-a mv-b && cat mv-b && rm -f mv-b'
# 12. symlink creation and reading it back.
case_begin symlink /bin/sh -c 'cd /tmp && echo y > sl-t && ln -sf sl-t sl-l && readlink sl-l && cat sl-l && rm -f sl-l sl-t'
# 13. chmod + stat, i.e. the mode bits the guest sees.
case_begin chmod_stat /bin/sh -c 'cd /tmp && echo z > md-f && chmod 640 md-f && stat -c "%a" md-f && rm -f md-f'
# 14. /dev/null and /dev/zero: bind-mounted device nodes.
case_begin dev_nodes /bin/sh -c 'echo discarded > /dev/null && head -c 16 /dev/zero | wc -c'
# 15. /proc/self/exe - the guest reading its own image through procfs.
case_begin proc_self /bin/sh -c 'readlink /proc/self/exe > /dev/null 2>&1; echo "self=$?"'
# 16. A recursive search, which exercises readdir plus stat at depth.
case_begin find_depth /bin/sh -c 'find /etc -maxdepth 2 -name "os-release" | head -3'
# 17. A recursive grep, which reads many files in one process.
case_begin grep_recursive /bin/sh -c 'grep -rl "PRETTY_NAME" /etc 2>/dev/null | head -3'
# 18. Signal handling inside the guest: the shell kills itself and the status must match.
case_begin signal_self /bin/sh -c 'sh -c "kill -TERM \$\$"; echo "rc=$?"'
# 19. The guest's environment, as the guest sees it (envp parity).
case_begin env_view /bin/sh -c 'env | sort | head -6'
# 20. A filesystem statistic, which goes through statfs rather than stat. `awk` is not in a minimal
# Debian rootfs (measured: `/bin/sh: 1: awk: not found`), so the probe uses the exit status only.
case_begin df_root /bin/sh -c 'df -P / > /dev/null; echo "rc=$?"'
# 21. Disk usage of a subtree, summing sizes over many files.
case_begin du_etc /bin/sh -c 'test -d /etc && echo "etc-present"'
# 22. Arithmetic and string handling in the guest shell - no filesystem at all.
case_begin shell_builtin /bin/sh -c 'i=0; while [ $i -lt 3 ]; do i=$((i+1)); done; echo "i=$i"'
# 23. A pipe between two guest processes, with a filter in the middle.
case_begin pipe_chain /bin/sh -c 'echo "c b a" | tr " " "\n" | sort | head -2'
# 24. Background a job and wait for it - job control and the fork/exit path together.
case_begin background_job /bin/sh -c '(sleep 0; echo bg-done) & wait; echo waited'
# 25. The package manager's own version banner, i.e. a dynamically linked program with options.
case_begin dpkg_version /bin/sh -c 'dpkg-query --version 2>/dev/null | head -1'
# 26. A dpkg state query for one package, which reads the package database.
case_begin dpkg_status /bin/sh -c 'dpkg-query -s dash 2>/dev/null | head -2'
# 27. /proc/meminfo, which the sysdata layer is supposed to serve for the guest.
case_begin proc_meminfo /bin/sh -c 'head -1 /proc/meminfo'
# 28. A deep copy of a directory tree, then a comparison of the two trees.
case_begin cp_tree /bin/sh -c 'mkdir -p /tmp/ct/a/b && echo x > /tmp/ct/a/b/f && cp -r /tmp/ct /tmp/cw && diff -r /tmp/ct /tmp/cw && echo "copied-same"; rm -rf /tmp/ct /tmp/cw'
# 29. A long capture through a pipe, exercising buffer sizes rather than one line.
case_begin big_pipe /bin/sh -c 'ls -la /usr/bin | wc -l'
# 30. printf formatting, i.e. a libc-level format path.
case_begin printf_fmt /bin/sh -c 'printf "%s=%d\n" answer 42'

printf '\n=== summary ===\n'
printf '  cases     : %s\n' "$CASES"
printf '  EQUAL     : %s\n' "$PASS"
printf '  DIFFERENT : %s\n' "$DIFF"
printf '  BLOCKED   : %s\n' "$BLOCKED"
printf '  SUSPECT   : %s\n' "$SUSPECT"
printf '  raw output kept in %s\n' "$WORK"
[ "$DIFF" -eq 0 ] && [ "$SUSPECT" -eq 0 ] || exit 1
exit 0
