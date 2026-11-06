# PROOT (Termux fork) — Deep Source Dissection for PDN

Scope: the parts a first pass did **not** read at statement level:
`src/syscall/`, `src/tracee/`, `src/extension/extension.c` + every extension
constructor, `src/execve/`, `src/loader/`.
The first pass's results (path prefix rewriting, CLI surface, nine extensions,
`/dev/null` over `/etc/ld.so.preload`, test layout) are **not** re-derived here;
they live in `research/PROOT_AND_PROOTDISTRO_SOURCE_DISSECTION.md`.

All line/file anchors are relative to the source tree root unless stated.

---

## 1. Method and what was read

**Repository under dissection**

```
/d/data/data/com.termux/files/home/.hermes/cache/scratch/proot-src
```

**Exact commit** (read-only; no git command that modifies state was run):

```
$ git -C /data/data/com.termux/files/home/.hermes/cache/scratch/proot-src log -1 --format=%H
52d65a4dc140ca554d241a1438ddd4f6b00c9813
$ git -C ... log -1 --format='%H %ci %s'
52d65a4dc140ca554d241a1438ddd4f6b00c9813 2026-11-01 12:14:48 +0000 Correct the urls to source code repo (home page)
```

CLI version string declared in-tree: `VERSION "5.1.0"` (`src/cli/proot.h:9`).

### Files read at statement level (with line counts)

Line counts are from `wc -l` (see §6, command C1).

**src/syscall/**

| file | lines | read |
|---|---|---|
| `enter.c` | 3069 | full (1–1050, 1051–2100, 2101–3069) |
| `exit.c` | 845 | full |
| `socket.c` | 622 | full |
| `seccomp.c` | 583 | full |
| `syscall.c` | 350 | full |
| `pipe_shadow.c` | 292 | full |
| `heap.c` | 213 | full |
| `chain.c` | 195 | full |
| `sysnum.c` | 161 | full |
| `rlimit.c` | 117 | full |
| `*.h`, `sysnums-*.h` | — | symbols referenced, tables not enumerated |

**src/tracee/**

| file | lines | read |
|---|---|---|
| `tracee.c` | 1053 | full |
| `event.c` | 962 | full |
| `mem.c` | 692 | full |
| `seccomp.c` | 673 | full |
| `reg.c` | 433 | full |
| `statx.c` | 146 | full |
| `tracee.h` | 437 | full |
| `abi.h` | 149 | full |
| `reg.h` | 58 | full |
| `mem.h`, `event.h`, `seccomp.h`, `statx.h` | — | not read (§5) |

**src/execve/**

| file | lines | read |
|---|---|---|
| `enter.c` | 731 | full |
| `exit.c` | 513 | full |
| `ldso.c` | 579 | full |
| `shebang.c` | 307 | full |
| `auxv.c` | 184 | full |
| `elf.c` | 178 | full |
| `execve.h` | 81 | full |
| `aoxp.c` | 439 | **not read** — only its exported API usage is cited (§5) |
| `aoxp.h`, `elf.h`, `ldso.h`, `shebang.h`, `auxv.h` | — | symbols referenced |

**src/loader/**

| file | lines | read |
|---|---|---|
| `loader.c` | 263 | full |
| `script.h` | 78 | full |
| `assembly-arm64.h` | 98 | full |
| `assembly-arm.h`, `assembly-x86.h`, `assembly-x86_64.h` | — | not read (same shape as arm64) |

**src/extension/**

| file | lines | read |
|---|---|---|
| `extension.c` | 169 | full |
| `extension.h` | 239 | full |
| `kompat/kompat.c` | 1049 | full |
| `fake_id0/fake_id0.c` | 1543 | full |
| `sysvipc/sysvipc.c` | 370 | full |
| `ashmem_memfd/ashmem_memfd.c` | 242 | full |
| `link2symlink/link2symlink.c` | 1788 | callback skeleton only (grep, §4/§5) |
| `fix_symlink_size/fix_symlink_size.c` | 119 | full |
| `hidden_files/hidden_files.c` | 196 | full |
| `port_switch/port_switch.c` | 256 | full |
| `mountinfo/mountinfo.c` | 208 | full |
| `fake_id0/{open,chmod,chown,stat,link,symlink,mk,rename,access,exec,unlink,utimensat,socket,sendmsg,getsockopt,chroot}.c` | — | **not read** (§5) |
| `sysvipc/sysvipc_{shm,sem,msg}.c`, `sysvipc_internal.h` | — | only `sysvipc.h` externs and grep hits cited (§5) |

**src/cli/** (needed to prove the enable matrix)

| file | lines | read |
|---|---|---|
| `cli.c` | 606 | relevant regions (120–179, 440–569) + grep |
| `proot.c` | 397 | full |
| `proot.h` | 350 | full |

### Commands run to establish the reading list and the commit

```
$ find src -name '*.c' -o -name '*.h' | sort
$ wc -l src/syscall/*.c src/tracee/*.c src/execve/*.c src/loader/*.c src/extension/*.c src/*.c | sort -n
$ git log -1 --format=%H
```

---

## 2. Subsystem map (file:line anchors)

```
proot (termux fork) @ 52d65a4dc140ca554d241a1438ddd4f6b00c9813
│
├─ src/cli/cli.c:444 main()
│    ├─ parse_config()                         cli.c:477
│    ├─ mountinfo extension                 cli.c:481 (default ON)
│    ├─ launch_process()                        cli.c:486
│    └─ event_loop()                            cli.c:505
│
├─ src/tracee/event.c
│    ├─ launch_process()                        event.c:95   (fork, TRACEME, SIGSTOP)
│    │     └─ enable_syscall_filtering()        event.c:145-146  (seccomp, unless PROOT_NO_SECCOMP)
│    ├─ event_loop()                            event.c:310  (waitpid __WALL)
│    └─ handle_tracee_event()                   event.c:468  (ptrace state machine)
│
├─ src/syscall/syscall.c:140 translate_syscall()  ← enter/exit driver
│    ├─ enter → syscall/enter.c:1887 translate_syscall_enter()
│    └─ exit  → syscall/exit.c:58    translate_syscall_exit()
│
├─ src/syscall/seccomp.c
│    ├─ proot_sysnums[]                         seccomp.c:331  (108 entries)
│    ├─ set_seccomp_filters()                   seccomp.c:253
│    └─ enable_syscall_filtering()              seccomp.c:500
│
├─ src/tracee/seccomp.c
│    ├─ handle_seccomp_event()                  seccomp.c:85   (SIGSYS from outer policy)
│    └─ handle_seccomp_event_common()           seccomp.c:148  (legacy→modern syscall rewrites)
│
├─ src/tracee/mem.c
│    ├─ read_data / write_data                  mem.c:338 / 218 (process_vm_* then PTRACE_PEEK/POKEDATA)
│    ├─ read_string                              mem.c:412
│    ├─ alloc_mem (push below SP)                mem.c:637
│    └─ ptrace_pokedata_or_via_stub              mem.c:98  (HAS_POKEDATA_WORKAROUND)
│
├─ src/execve
│    ├─ enter.c:596 translate_execve_enter()     (ELF parse, PIC base, loader swap)
│    ├─ exit.c:411  translate_execve_exit()
│    ├─ exit.c:174  transfer_load_script()       (load script → tracee stack)
│    ├─ ldso.c:451  rebuild_host_ldso_paths()    (mixed-mode LD_LIBRARY_PATH)
│    ├─ shebang.c:208 expand_shebang()
│    └─ auxv.c:117 fetch_elf_aux_vectors() / auxv.c:157 push_elf_aux_vectors()
│
├─ src/loader
│    ├─ loader.c:111 _start()                    (interprets LoadStatement stream)
│    └─ script.h:29 LoadStatement + LOAD_ACTION_* 0..6
│
└─ src/extension/extension.c
     ├─ initialize_extension()                   extension.c:103
     ├─ inherit_extensions()                      extension.c:128
     └─ notify_extensions()                       extension.h:210
```

---

## 3. Mechanisms PDN must understand

Each item: the fact, the proof excerpt (≤15 lines), and its anchor.

### 3.1 The ptrace state machine: one loop, `waitpid(__WALL)`, restart policy

`event_loop()` is the whole runtime. It installs handlers, reaps stopped tracees,
and restarts each one. Register/chained/seccomp state decides `PTRACE_CONT` vs
`PTRACE_SYSCALL`.

`src/tracee/event.c:389-412`
```c
	while (1) {
		int tracee_status;
		Tracee *tracee;
		int saved_errno;
		int signal;
		pid_t pid;

		/* This is the only safe place to free tracees.  */
		free_terminated_tracees();

		/* Release shadow pipe read ends that are of no use
		 * anymore.  */
		shadow_pipes_reap();
		...
		pid = waitpid(-1, &tracee_status, __WALL);
```

Restart policy — **seccomp ON + no exit stage needed ⇒ `PTRACE_CONT`** (the
acceleration), otherwise `PTRACE_SYSCALL`:

`src/tracee/event.c:488-499`
```c
	sysexit_necessary = tracee->sysexit_pending
				|| tracee->chain.syscalls != NULL
				|| tracee->restore_original_regs_after_seccomp_event;
	if (tracee->restart_how == 0) {
		if (tracee->seccomp == ENABLED && !sysexit_necessary)
			tracee->restart_how = PTRACE_CONT;
		else
			tracee->restart_how = PTRACE_SYSCALL;
	}
```

`seccomp` is a per-tracee tri-state `{DISABLED, DISABLING, ENABLED}`
(`src/tracee/tracee.h:332`). `restart_tracee()` performs the actual `ptrace()`
and clears `restart_how`:

`src/tracee/event.c:942-959`
```c
bool restart_tracee(Tracee *tracee, int signal)
{
	int status;

	/* Put in the "stopped"/"waiting for ptracee" state?.  */
	if (tracee->as_ptracer.wait_pid != 0 || signal == -1)
		return false;

	/* Restart the tracee and stop it at the next instruction, or
	 * at the next entry or exit of a system call. */
	assert(tracee->restart_how != 0);
	status = ptrace(tracee->restart_how, tracee->pid, NULL, signal);
```

The event dispatch itself (`handle_tracee_event`) distinguishes SIGTRAP,
`SIGTRAP|0x80` (syscall stop), `SIGTRAP|PTRACE_EVENT_*<<8`, SIGSTOP, SIGSYS;
the SIGTRAP case installs `PTRACE_O_TRACESECCOMP` and the TRACE* options
(`event.c:526-578`).

### 3.2 Seccomp acceleration: a generated BPF program from `proot_sysnums[]`

The filter is *built from a static list* and, for the exit stage only, marked
with `FILTER_SYSEXIT`. The program shape is: per arch → per traced syscall emit
`SECCOMP_RET_TRACE+flag` → `SECCOMP_RET_ALLOW` → `SECCOMP_RET_KILL`.

`src/syscall/seccomp.c:311-320`
```c
	status = prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
	if (status < 0)
		goto end;

	/* To output this BPF program for debug purpose:
	 *
	 *     write(2, program.filter, program.len * sizeof(struct sock_filter));
	 */

	status = prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &program);
```

The list is `proot_sysnums[]`, 108 entries, terminator `FILTERED_SYSNUM_END`
(`seccomp.c:331-445`). Selection of flags per syscall is `0` (enter only) or
`FILTER_SYSEXIT` (both stages). Examples:

`src/syscall/seccomp.c:332-338, 401-411`
```c
	{ PR_accept,		FILTER_SYSEXIT },
	{ PR_accept4,		FILTER_SYSEXIT },
	{ PR_access,		0 },
	...
	{ PR_open,		0 },
	...
	{ PR_readlink,		FILTER_SYSEXIT },
	{ PR_readlinkat,	FILTER_SYSEXIT },
```

Enabled in the launch child **before the first execve**, unless
`PROOT_NO_SECCOMP` is set:

`src/tracee/event.c:143-146`
```c
		/* Improve performance by using seccomp mode 2, unless
		 * this support is explicitly disabled.  */
		if (getenv("PROOT_NO_SECCOMP") == NULL)
			(void) enable_syscall_filtering(tracee);
```

Extensions add their own `filtered_sysnums[]` to the same list
(`seccomp.c:516-526`), so enabling an extension changes the compiled filter.
The exact same lists are re-read at runtime by `filtered_sysnum_flags()`
(`seccomp.c:540`) for kernels whose `PTRACE_GETEVENTMSG` loses the flag word
(`event.c:735-750`).

There are **two seccomp stop orders** and proot routes around both, detected
once (`seccomp_after_ptrace_enter`):

`src/tracee/event.c:691-697`
```c
			if (!seccomp_detected) {
				tracee->seccomp = ENABLED;
				seccomp_detected = true;
				seccomp_after_ptrace_enter = !IS_IN_SYSENTER(tracee);
				VERBOSE(tracee, 1, "ptrace acceleration (seccomp mode 2, %s syscall order) enabled",
						seccomp_after_ptrace_enter ? "new" : "old");
			}
```

`PROOT_ASSUME_NEW_SECCOMP` forces the "new order" flag
(`event.c:476-479`).

### 3.3 The syscall driver: enter/exit, and the "avoider" (`PR_void`) trick

`translate_syscall()` is called for every traced syscall stop. It runs the
enter or exit translator, and when an enter translation yields an error it
**replaces the syscall number with `PR_void`** and pokes the error into the
result register at enter time.

`src/syscall/syscall.c:176-198`
```c
		if (tracee->chain.syscalls == NULL) {
			save_current_regs(tracee, ORIGINAL);
			status = translate_syscall_enter(tracee);
			save_current_regs(tracee, MODIFIED);
		}
		else {
			if (tracee->chain.sysnum_workaround_state != SYSNUM_WORKAROUND_PROCESS_REPLACED_CALL) {
				status = notify_extensions(tracee, SYSCALL_CHAINED_ENTER, 0, 0);
			}
			tracee->restart_how = PTRACE_SYSCALL;
		}

		/* Remember the tracee status for the "exit" stage and
		 * avoid the actual syscall if an error was reported
		 * by the translation/extension. */
		if (status < 0) {
			set_sysnum(tracee, PR_void);
			poke_reg(tracee, SYSARG_RESULT, (word_t) status);
			tracee->status = status;
```

`PR_void` maps to `SYSCALL_AVOIDER` (`syscall/sysnum.c:110-111`), whose value is
arch-specific:

`src/arch.h:34, 111-112, 159-160`
```c
#define SYSCALL_AVOIDER ((word_t) -2)
...
    /* The syscall number has to be valid on ARM, so use tuxcall(2) as
     * the "void" syscall since it has no side effects.  */
    #undef SYSCALL_AVOIDER
    #define SYSCALL_AVOIDER ((word_t) 222)
...
    /* Syscall -2 appears to cause some odd side effects, use -1. */
    #undef SYSCALL_AVOIDER
    #define SYSCALL_AVOIDER ((word_t) -1)
```

On ARM the avoider is *in range* (222 = tuxcall) and reaches the kernel; proot
therefore tracks whether the kernel **cancels** a voided syscall and asks for
the exit stop only when needed:

`src/syscall/syscall.c:131-138`
```c
static bool kernel_cancels_voided_syscall(void)
{
	/* A 32-bit tracee running on a 64-bit kernel needs no special
	 * case here: the avoider reaches that kernel truncated to its
	 * 32 least significant bits, which are read back as a negative
	 * syscall number just the same.  */
	return (long) SYSCALL_AVOIDER < 0;
}
```

### 3.4 Per-syscall enter translation: path arguments are rewritten in place

`translate_syscall_enter()` is a 1,100-line switch (`enter.c:1887`–`3060`).
Two helpers do the work — full path translation and "parent only" translation:

`src/syscall/enter.c:148-163`
```c
static int translate_path2(Tracee *tracee, int dir_fd, char path[PATH_MAX], Reg reg, Type type)
{
	char new_path[PATH_MAX];
	int status;

	/* Special case where the argument was NULL. */
	if (path[0] == '\0')
		return 0;

	/* Translate the original path. */
	status = translate_path(tracee, new_path, dir_fd, path, type != SYMLINK);
	if (status < 0)
		return status;

	return set_sysarg_path(tracee, new_path, reg);
}
```

Creation-style destinations (`mkdir`, `mkdirat`, `mknod`…, `link`/`symlink`
newpaths) translate the **parent only** — deliberately not probing the new name
(`enter.c:171-220`, comment at `enter.c:165-170`). `set_sysarg_path` allocates
in the tracee's memory and repoints the register (`syscall/syscall.c:72-101`).

### 3.5 Fully emulated syscalls (no real kernel call)

- **`getcwd`**: voided at enter, reconstructed at exit.
  `src/syscall/enter.c:1950-1954`
  ```c
	case PR_getcwd:
		poke_reg(tracee, SYSARG_RESULT, 0);
		set_sysnum(tracee, PR_void);
		status = 0;
		break;
  ```
  Reconstructed at `src/syscall/exit.c:95-128` from `tracee->fs->cwd`.
- **`chdir`/`fchdir`**: path validated with `lstat`, cwd recomputed and stored;
  syscall voided (`enter.c:1956-2026`), scyexit forces `0` (`exit.c:276-290`).
- **`unshare`/`setns`**: returned success, no-op; `unshare(CLONE_NEWNET)` sets
  `tracee->fake_netns` (`enter.c:2510-2522`).
- **`umount`/`umount2`**: `apply_emulated_umount()` + void (`enter.c:2524-2530`).
- **`mount`/`pivot_root`**: emulated as PRoot bindings, voided
  (`enter.c:2586-2598`). The emulation is `emulate_mount()`
  (`enter.c:292-331`) and `emulate_pivot_root()` (`enter.c:340-473`).
- **`clone`/`clone3`**: `CLONE_NEW*` bits stripped, fork family restarted with
  the exit stage (`enter.c:2542-2581`).
- **`fsopen`/`fsconfig`/`fsmount`/`fspick`/`move_mount`/`mount_setattr`/
  `open_tree_attr`**: forced `-ENOSYS` so libmount falls back to `mount(2)`
  (`enter.c:2607-2615`).
- **`prctl(PR_SET_DUMPABLE)`**: voided so the tracee can't break tracer memory
  access (`enter.c:2919-2926`).
- **`prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER)`**: blocked with `-EPERM` when
  the kernel lacks `PTRACE_O_TRACESECCOMP` (`enter.c:2936-2948`).
- **`prctl(PR_GET_NO_NEW_PRIVS)`**: answered from `tracee->no_new_privs`
  (`enter.c:2957-2972`).
- **`memfd_create`**: `-EACCES` for `JITCode:`, `opcache_lock`,
  `lib/apk/exec/` names (`enter.c:3010-3038`).

`execveat` is rewritten to `execve` when `dirfd == AT_FDCWD`, else `-ENOSYS`:

`src/syscall/enter.c:1922-1934`
```c
	case PR_execveat:
		if ((int) peek_reg(tracee, CURRENT, SYSARG_1) == AT_FDCWD) {
			set_sysnum(tracee, PR_execve);
			poke_reg(tracee, SYSARG_1, peek_reg(tracee, CURRENT, SYSARG_2));
			...
		} else {
			note(tracee, ERROR, SYSTEM, "execveat() with non-AT_FDCWD fd is not currently supported");
			status = -ENOSYS;
			break;
		}
```

### 3.6 Exit translation: output arguments, cwd, `/dev/shm`, ioctl, netlink

`translate_syscall_exit()` (`exit.c:58`) dispatches on the **ORIGINAL** sysnum.
Notable PDN-relevant behaviours:

- **`/dev/shm` claimed to be tmpfs** via `statfs`/`statfs64`: the magic
  `0x01021994` is written over the returned structure when the path is under the
  translated `/dev/shm`:

`src/syscall/exit.c:744-759`
```c
		Comparison comparison = compare_paths(devshm_path, statfs_path);
		if (comparison == PATHS_ARE_EQUAL || comparison == PATH1_IS_PREFIX) {
			VERBOSE(tracee, 5, "Updating statfs() result to fake tmpfs /dev/shm");
			...
			word_t stat_addr = peek_reg(tracee, ORIGINAL, syscall_number == PR_statfs64 ? SYSARG_3 : SYSARG_2);
			int write_status = write_data(tracee, stat_addr, "\x94\x19\x02\x01", 4);
```

  The same fake is applied on the SIGSYS path
  (`src/tracee/seccomp.c:352-358`).

- **`readlink`/`readlinkat`**: content detranslated; truncated reads re-read with
  a full buffer; `READLINK_PROC_FD` extension hook for `/proc/<pid>/fd/<n>`
  (`exit.c:369-519`).
- **`rename`/`renameat`**: if the moved node is the cwd, the cwd is rewritten
  (`exit.c:292-367`).
- **`statx`**: replaced wholesale by a stat-based emulation
  (`exit.c:768-770` → `tracee/statx.c:7`).
- **`socketcall`/`accept`/`getsockname`/`getpeername`**: sockaddr detranslation
  (`exit.c:130-156`, `189-270`); **`sendmmsg`** results copied back
  (`exit.c:158-171`, `223-243`).
- **`FICLONE` ioctl** returning `-EACCES` is rewritten to `-EOPNOTSUPP`
  (`exit.c:772-777`).
- **Netlink**: `PR_socket` exit records substituted/real NETLINK_ROUTE fds
  (`exit.c:779-826`); `recvfrom`/`recvmsg` exit runs `handle_netlink_reply_exit`
  (`exit.c:828-833`).

### 3.7 `brk` is emulated with mmap/mremap

`translate_brk_enter` (`syscall/heap.c:54`) rewrites the first `brk` into an
anonymous `mmap2`/`mmap` and later ones into `mremap`:

`src/syscall/heap.c:133-142`
```c
	new_heap_size = new_brk_address - tracee->heap->base;
	old_heap_size = tracee->heap->size;

	/* Actually resizing.  */
	set_sysnum(tracee, PR_mremap);
	poke_reg(tracee, SYSARG_1 /* old_address */, tracee->heap->base - heap_offset);
	poke_reg(tracee, SYSARG_2 /* old_size    */, old_heap_size + heap_offset);
	poke_reg(tracee, SYSARG_3 /* new_size    */, new_heap_size + heap_offset);
	poke_reg(tracee, SYSARG_4 /* flags       */, 0);
	poke_reg(tracee, SYSARG_5 /* new_address */, 0);
```

`translate_brk_exit` (`heap.c:150`) maps the underlying result back to a brk
return value and can permanently disable the emulation (`heap.c:201-206`).

### 3.8 Chained syscalls: proot inserts syscalls the tracee never asked for

`register_chained_syscall()` appends to a per-tracee FIFO (`chain.c:87`).
`chain_next_syscall()` (`chain.c:103`) pokes the next syscall's args, moves the
instruction pointer back one trap and sets `restart_how = PTRACE_SYSCALL`:

`src/syscall/chain.c:133-152`
```c
	/* The list of chained syscalls is a FIFO.  */
	syscall = STAILQ_FIRST(tracee->chain.syscalls);
	STAILQ_REMOVE_HEAD(tracee->chain.syscalls, link);

	poke_reg(tracee, SYSARG_1, syscall->sysargs[0]);
	...
	sysnum = detranslate_sysnum(get_abi(tracee), syscall->sysnum);
	poke_reg(tracee, SYSTRAP_NUM, sysnum);

	/* Move the instruction pointer back to the original trap.  */
	instr_pointer = peek_reg(tracee, CURRENT, INSTR_POINTER);
	poke_reg(tracee, INSTR_POINTER, instr_pointer - get_systrap_size(tracee));

	/* Break after exit from syscall, there may be another one in chain */
	tracee->restart_how = PTRACE_SYSCALL;
```

The final result of the original syscall can be forced
(`force_chain_final_result`, `chain.c:159`) — used by `kompat` fd-flag emulation
and by `mount`/`link` emulations elsewhere.

### 3.9 Memory access: `process_vm_*` fast path with a ptrace fallback, and the pokedata workaround

`read_data` / `write_data` (`mem.c:338` / `mem.c:218`) first try
`process_vm_readv` / `process_vm_writev`, then fall back to word-at-a-time
`PTRACE_PEEKDATA` / `PTRACE_POKEDATA`. `read_string` reads in page-sized chunks
to survive a partially-valid remote mapping (`mem.c:424-495`).

On aarch64 builds a broken `PTRACE_POKEDATA` is detected on the first use and
replaced by executing an in-tracee stub:

`src/tracee/mem.c:109-121`
```c
	if (!pokedata_workaround_checked) {
		pokedata_workaround_needed = (status != 0);
		pokedata_workaround_checked = true;
		if (pokedata_workaround_needed) {
			VERBOSE(tracee, 1, "Detected broken PTRACE_POKEDATA - enabling workaround");
		}
	}
	if (pokedata_workaround_needed && tracee->is_aarch32) {
		note(tracee, ERROR, INTERNAL, "POKEDATA workaround is not supported on AArch32");
		status = -1;
		errno = EIO;
	}
```

`alloc_mem` pushes data **below** the tracee's stack pointer and is enter-stage
only (`mem.c:637-673`); the stack pointer is restored at the end of scyexit
unless the enter stage asked to keep it (see §3.10 and `syscall/syscall.c:240-244`).

### 3.10 The two "keep the pushed args" / "force the exit stage" switches

Both are `{sysexit_pending = true; restart_how = PTRACE_SYSCALL;}` and exist
because seccomp's `PTRACE_CONT` fast path would otherwise let the kernel run the
syscall without a sysexit stop. `force_fork_sysexit` works around a kernel that
re-reports the clone enter stop; `keep_pushed_args` keeps the lowered stack
pointer valid across a signal-driven restart.

`src/syscall/enter.c:118-139`
```c
static void force_fork_sysexit(Tracee *tracee)
{
	tracee->sysexit_pending = true;
	tracee->restart_how = PTRACE_SYSCALL;
}
...
static void keep_pushed_args(Tracee *tracee)
{
	tracee->sysexit_pending = true;
	tracee->restart_how = PTRACE_SYSCALL;
}
```

### 3.11 SIGSYS from an *outer* seccomp policy: legacy→modern rewrites

Android's parent process installs a filter that `SECCOMP_RET_TRAP`s many
syscalls. proot catches SIGSYS with `si_code == SYS_SECCOMP`
(`event.c:837-891`) and `handle_seccomp_event()` (`tracee/seccomp.c:85`) rewrites
the rejected legacy syscall to a modern equivalent and restarts it, or answers
it synthetically:

`src/tracee/seccomp.c:225-243`
```c
	case PR_mount:
		apply_emulated_mount(tracee);
		set_result_after_seccomp(tracee, 0);
		break;

	case PR_pivot_root:
		apply_emulated_pivot_root(tracee);
		set_result_after_seccomp(tracee, 0);
		break;

	case PR_umount:
	case PR_umount2:
		apply_emulated_umount(tracee);
		set_result_after_seccomp(tracee, 0);
		break;

	case PR_unshare:
	case PR_setns:
		set_result_after_seccomp(tracee, 0);
		break;
```

The default branch returns `-ENOSYS` (`seccomp.c:666-669`). A SIGSYS on the
avoider syscall is swallowed (`event.c:865-887`).

### 3.12 `statx` is fully emulated with `stat`/`lstat`/`fstat`

`handle_statx_syscall()` (`tracee/statx.c:7`) detects `AT_SYMLINK_NOFOLLOW`,
translates the path, calls the host `[l]stat`, copies the fields selected by the
`mask`, and notifies extensions via `STATX_SYSCALL`:

`src/tracee/statx.c:49-58`
```c
	if (from_sigsys || peek_reg(tracee, CURRENT, SYSARG_RESULT) != 0) {
		/* Call [l]stat() on translated path */
		if (do_fstat) {
			char link[32] = {}; /* 32 > sizeof("/proc//cwd") + sizeof(#ULONG_MAX) */
			snprintf(link, sizeof(link), "/proc/%d/fd/%d", tracee->pid, (int) dirfd);
			status = stat(link, &stat_buf);
		} else if (do_lstat) {
			status = lstat(state.host_path, &stat_buf);
		} else {
			status = stat(state.host_path, &stat_buf);
		}
```

Called at scyexit (`exit.c:768`) and on the SIGSYS path (`tracee/seccomp.c:601-606`).

### 3.13 `execve` injection: proot replaces the guest program with its own loader

`translate_execve_enter()` (`execve/enter.c:596`):

1. reads the guest path, remembers `raw_path` for `AT_EXECFN`/comm;
2. `expand_shebang()` (`shebang.c:208`) rewrites scripts into
   `interpreter [arg] script` argv;
3. records `host_exe`/`new_exe` (the future `/proc/self/exe`, extension-hookable
   via `EXECVE_PROC_EXE`, `enter.c:644-657`);
4. with `-q`, `expand_runner()` (`enter.c:368`) rewrites argv for QEMU;
5. parses the ELF (`extract_load_info`, `enter.c:273`) and the interpreter;
6. computes PIC load addresses (`compute_load_addresses`, `enter.c:333`);
7. **replaces `SYSARG_1` with the loader's path**:

`src/execve/enter.c:718-728`
```c
	/* Execute the loader instead of the program.  */
	loader_path = get_loader_path(tracee);
	if (loader_path == NULL)
		return -ENOENT;

	status = set_sysarg_path(tracee, loader_path, SYSARG_1);
	if (status < 0)
		return status;

	/* Mask to its ptracer syscalls performed by the loader.  */
	tracee->as_ptracee.ignore_loader_syscalls = true;
```

The loader is an **embedded binary extracted to a temp file** at runtime; its
path is cached per class (32/64):

`src/execve/enter.c:573-587`
```c
	static char *loader_path = NULL;

#if defined(HAS_LOADER_32BIT)
	static char *loader32_path = NULL;

	if (IS_CLASS32(tracee->load_info->elf_header)) {
		loader32_path = loader32_path ?: getenv("PROOT_LOADER_32") ?: extract_loader(tracee, true);
		return loader32_path;
	}
	else
#endif
	{
		loader_path = loader_path ?: getenv("PROOT_LOADER") ?: extract_loader(tracee, false);
		return loader_path;
	}
```

`extract_loader()` (`enter.c:483`) writes `_binary_loader_exe_start.._end` to
`open_temp_file(NULL, "prooted")`, `fchmod`s it `u+rx`, and resolves the path via
`/proc/self/fd`. The temp-dir choice is overridable with `PROOT_TMP_DIR`
(`enter.c:531-535`).

**If `skip_proot_loader` is set (QEMU without `PROOT_USE_LOADER_FOR_QEMU`), the
loader is bypassed entirely** and `SYSARG_1` is set to the host path instead
(`enter.c:670-679`); the same flag short-circuits `translate_execve_exit`
(`execve/exit.c:418-422`).

At the **exit** stage, `transfer_load_script()` (`execve/exit.c:174`) serializes
`load_info->mappings` into `LoadStatement`s and pushes the whole script +
strings below the current stack pointer, then points `USERARG_1` at it:

`src/execve/exit.c:362-366`
```c
	poke_reg(tracee, STACK_POINTER, stack_pointer - buffer_size);
	poke_reg(tracee, USERARG_1, stack_pointer - buffer_size);

	/* Copy everything in the tracee's memory at once.  */
	status = write_data(tracee, stack_pointer - buffer_size, buffer, buffer_size);
```

The stack layout it produces is documented inline (`execve/exit.c:370-395`).

### 3.14 The loader's `_start` interprets the load script

`src/loader/loader.c:111` `_start(void *cursor)` walks `LoadStatement`s. It opens
the program/interpreter files, `mmap`s each `Mapping` `MAP_FIXED`, optionally
makes the stack executable, patches `AT_PHDR/AT_PHENT/AT_PHNUM/AT_ENTRY/AT_BASE/
AT_EXECFN` on the initial stack, and then branches (or `execve`s if traced).

`src/loader/loader.c:208-253`
```c
			/* Adjust auxv[].  */
			do {
				switch (cursor2[0]) {
				case AT_PHDR:
					cursor2[1] = stmt->start.at_phdr;
					break;
				...
				case AT_EXECFN:
					cursor2[1] = at_execfn;
					break;
				...
				cursor2 += 2;
			} while (cursor2[0] != AT_NULL);

			/* Note that only 2 arguments are actually necessary... */
			name = basename(stmt->start.at_execfn);
			SYSCALL(PRCTL, 3, PR_SET_NAME, name, 0);

			if (unlikely(traced))
				SYSCALL(EXECVE, 6, 1, stmt->start.stack_pointer, stmt->start.entry_point, 2, 3, 4);
			else
				BRANCH(stmt->start.stack_pointer, stmt->start.entry_point);
```

`LOAD_ACTION_*` values and the `LoadStatement` layout:
`src/loader/script.h:29-76`. On aarch64 the final `BRANCH` is inline asm that
restores `sp`, zeroes `x0` (rtld_fini) and `br`s
(`src/loader/assembly-arm64.h:30-44`).

`AT_EXECFN` handling is deliberately split: the loader patches the in-memory
auxv, and proot additionally records `argv[0]`'s address to patch
`prctl(PR_GET_AUXV)` and `read()` of `/proc/self/auxv` later
(`execve/exit.c:208-214`, `syscall/exit.c:586-687`).

### 3.15 `PTRACE_EVENT_EXEC` bookkeeping, SIGPIPE, and kill-on-exit

- A thread other than the group leader calling `execve` is reconciled by
  `get_execve_tracee()` (`tracee/tracee.c:408`), invoked from the event loop
  (`event.c:431-436`).
- The launch child resets `SIGPIPE` to `SIG_DFL` before `TRACEME`
  (`event.c:126`).
- `kill-all-on-exit`: `handle_option_kill_on_exit` sets
  `tracee->killall_on_exit` (`cli/proot.c:195-199`); it is consumed by
  `terminate_tracee()` (`tracee/tracee.c:350-361`) and `event_loop`'s `atexit`
  (`event.c:317`).
- pid-reuse and children whose fork event lacked a PID are handled by
  `new_child`/`attach_child`/`resolve_pending_child`/`adopt_held_children`
  (`tracee/tracee.c:662`, `716`, `613`, `574`).

---

## 4. Extension enable matrix

`Extension` objects are created only by `initialize_extension()`
(`src/extension/extension.c:103`); there is **no other registration path**. The
only built-in list is the `extern`-declared callbacks in `extension.h:228-237`.
All call sites were enumerated by grep (see §6, C4):

```
$ grep -rn "initialize_extension" src/cli/
src/cli/cli.c:482:   initialize_extension(tracee, mountinfo_callback, NULL);
src/cli/proot.c:168: status = initialize_extension(tracee, kompat_callback, value);
src/cli/proot.c:186: (void) initialize_extension(tracee, fake_id0_callback, value);
src/cli/proot.c:289: status = initialize_extension(tracee, link2symlink_callback, NULL);
src/cli/proot.c:302: status = initialize_extension(tracee, ashmem_memfd_callback, NULL);
src/cli/proot.c:315: status = initialize_extension(tracee, sysvipc_callback, NULL);
src/cli/proot.c:324: (void) initialize_extension(tracee, fix_symlink_size_callback, NULL);
src/cli/proot.c:330: (void) initialize_extension(tracee, hidden_files_callback, NULL);
src/cli/proot.c:336: (void) initialize_extension(tracee, port_switch_callback, NULL);
```

| Extension | Default | Condition to enable | Enabling CLI option(s) | Source (call site / option table) | `filtered_sysnums` set at |
|---|---|---|---|---|---|
| `mountinfo` | **ON** | enabled unless env `PROOT_NO_MOUNTINFO` is set | *(none; automatic)* | `cli/cli.c:481-482` | `mountinfo.c:194` (no `filtered_sysnums`; uses `TRANSLATED_PATH`) |
| `kompat` | OFF | `-k`/`--kernel-release` given | `-k string`, `--kernel-release=string` | `cli/proot.c:168`; `proot.h:196-208` | `kompat.c:1010` |
| `fake_id0` | OFF | `-0`, `--root-id`, `-i`, `--change-id`, or `-S` given | `-0`, `--root-id`, `-i string`, `--change-id=string`, `-S path` | `cli/proot.c:186`; `proot.h:209-235`, `324-344` | `fake_id0.c:1372` |
| `link2symlink` | OFF | `-l`/`--link2symlink` given | `-l`, `--link2symlink` | `cli/proot.c:289`; `proot.h:237-246` | `link2symlink.c:1787` |
| `sysvipc` | OFF | `--sysvipc` given | `--sysvipc` | `cli/proot.c:315`; `proot.h:247-256` | `sysvipc.c:175` |
| `ashmem_memfd` | OFF | `--ashmem-memfd` given; **built only on `__ANDROID__`/`__BIONIC__`** | `--ashmem-memfd` | `cli/proot.c:302`; `proot.h:257-266`; guarded by `proot.h:64-66` | `ashmem_memfd.c:182` |
| `fix_symlink_size` | OFF | `-L` given | `-L` | `cli/proot.c:324`; `proot.h:283-290` | `fix_symlink_size.c:108` |
| `hidden_files` | OFF | `-H` given | `-H` | `cli/proot.c:330`; `proot.h:267-274` | `hidden_files.c:184` |
| `port_switch` | OFF | `-p` given | `-p` | `cli/proot.c:336`; `proot.h:275-282` | `port_switch.c:43` |

Notes that PDN currently assumes and that this pass **confirms or corrects**:

- **Exactly one extension is on by default: `mountinfo`.** It is registered after
  `parse_config()` and before `launch_process()`, so it also applies to the
  initial command:

`src/cli/cli.c:476-482`
```c
	/* Pre-configure the first tracee.  */
	status = parse_config(tracee, argc, argv);
	if (status < 0)
		goto error;

	if (NULL == getenv("PROOT_NO_MOUNTINFO"))
		initialize_extension(tracee, mountinfo_callback, NULL);
```

  `mountinfo` only rewrites `/proc/<pid>/mountinfo` opens
  (`mountinfo.c:191-203`, `41-189`) — it adds no syscall to the seccomp filter.

- **`sysvipc` is default OFF.** Its callback is only created by `handle_option_sysvipc`
  (`cli/proot.c:310-320`). It spawns a helper by re-execing proot with
  `--shm-helper`, handled at the very top of `main`:

`src/cli/cli.c:457-459`
```c
	if (argc == 2 && strcmp(argv[1], "--shm-helper") == 0) {
		sysvipc_shm_helper_main();
	}
```
  (re-exec site: `extension/sysvipc/sysvipc_shm.c:156`). Its libandroid-shmem
  sharing is on by default *within* the extension, unless
  `PROOT_DONT_SHARE_LIBANDROID_SHMEM` is set (`sysvipc.c:166-170`).

- **`link2symlink` is default OFF.** Only `-l`/`--link2symlink`.

- **`ashmem_memfd` is default OFF and Android-only.** The option table entry and
  the callback are both compiled out on non-Android builds
  (`proot.h:257-266`, `ashmem_memfd.c:1`, `242`). It probes `memfd_create` support
  by forking a child that calls `__NR_memfd_create` (`ashmem_memfd.c:39-86`);
  `PROOT_ASSUME_MEMFD_UNSUPPORTED=1|0` overrides the probe.

- **`fake_id0` is default OFF** and is created by `-0`, `-i`, `--root-id`,
  `--change-id`, or `-S`. `-0` is literally `-i 0:0`
  (`cli/proot.c:190-193`); `-S` = `-0` + `-r` + `recommended_su_bindings`
  (`cli/proot.c:267-282`).

- `kompat`'s behaviour is netkernel-dependent and further gated by
  `PROOT_FORCE_KOMPAT` (`kompat.c:879`).

- **`inheritance` model** (matters for PDN's process model): the parent decides
  per child via `INHERIT_PARENT` — `<0` not inheritable, `==0` inherit with
  shared config, `>0` inherit and call `INHERIT_CHILD`
  (`extension.c:128-168`; semantics documented at `extension.h:98-113`).
  `kompat`, `fake_id0`, `sysvipc`, `ashmem_memfd`, `link2symlink` return `1`
  (deep copy). `mountinfo`, `hidden_files`, `port_switch`, `fix_symlink_size`
  define no `INHERIT_PARENT`, so their callback returns `0` for that event →
  inherited with a **shared** config.

### Environment variables that alter default behaviour (all read with `getenv`)

| var | effect | source |
|---|---|---|
| `PROOT_NO_SECCOMP` | skip installing the seccomp filter (accelerator off) | `tracee/event.c:145`; help note `cli/cli.c:140-143` |
| `PROOT_NO_MOUNTINFO` | do not enable `mountinfo` | `cli/cli.c:481` |
| `PROOT_VERBOSE` | initial verbose level | `cli/cli.c:469-472` |
| `PROOT_ASSUME_NEW_SECCOMP` | force "seccomp after ptrace-enter" order | `tracee/event.c:477` |
| `PROOT_TMP_DIR` | alternate temp dir (loader/auxv/mountinfo/glue) | `execve/enter.c:531-535` |
| `PROOT_LOADER` / `PROOT_LOADER_32` | override the embedded loader path | `execve/enter.c:568,571,579,585` |
| `PROOT_USE_LOADER_FOR_QEMU` | keep the loader in the QEMU path | `execve/enter.c:391` |
| `PROOT_FORCE_KOMPAT` | treat actual kernel as unknown | `kompat.c:879` |
| `PROOT_ASSUME_MEMFD_UNSUPPORTED` | force memfd support probe result | `ashmem_memfd.c:40` |
| `PROOT_DONT_SHARE_LIBANDROID_SHMEM` | disable libandroid shmem sharing | `sysvipc.c:167` |
| `PROOT_FORCE_FOREIGN_BINARY` | treat every binary as foreign | `execve/elf.c:159` |

---

## 5. Open questions / UNVERIFIED

Marked UNVERIFIED where no statement-level read or command output backs a claim.

1. **UNVERIFIED — `src/execve/aoxp.c` (439 lines).** Not read. Only its API is
   cited (`fetch_array_of_xpointers`, `resize_array_of_xpointers`,
   `write_xpointee(s)`, `read_xpointee_as_string`, `push_array_of_xpointers`).
   *Would settle it:* read `aoxp.c` in full. PDN needs its `<PATH_MAX` /
   `#!`-argv packing rules only if it reimplements argv rewriting.
2. **UNVERIFIED — `src/ptrace/{ptrace.c,user.c,wait.c}`.** The tracer-side
   ptrace *emulation* (`as_ptracer`, `as_ptracee`, zombies) is exercised by the
   event loop (`event.c:450-452`) and `tracee.c`, but the emulation internals
   were not read. *Would settle it:* read the three files (out of the assigned
   scope but adjacent).
3. **UNVERIFIED — link2symlink body.** Only its callback skeleton and
   `INITIALIZATION`/`filtered_sysnums` were read (via grep, see §6 C6). The
   `.l2s.` naming, `PREFIX`/`DELETED_SUFFIX` (`link2symlink.c:29-35`), and the
   meta-file machinery were not traced.
4. **UNVERIFIED — fake_id0 per-syscall submodules**
   (`open.c`, `chmod.c`, `chown.c`, `stat.c`, `link.c`, `symlink.c`, `mk.c`,
   `rename.c`, `access.c`, `exec.c`, `unlink.c`, `utimensat.c`, `socket.c`,
   `sendmsg.c`, `getsockopt.c`, `chroot.c`). Only the top-level
   `fake_id0.c` (1543 lines) and its `filtered_sysnums[]` (`fake_id0.c:323-413`)
   were read.
5. **UNVERIFIED — sysvipc shm/sem/msg implementations.**
   `sysvipc_{shm,sem,msg}.c` were not read; only the callback layer
   (`sysvipc.c`, 370 lines) and the helper entry point
   (`sysvipc_shm.c:820 sysvipc_shm_helper_main`, `sysvipc_shm.c:156 execl`).
6. **UNVERIFIED — build-time feature macros actually used by the Termux package.**
   `HAVE_SECCOMP_FILTER` (`syscall/seccomp.c:26`), `HAVE_PROCESS_VM`
   (`tracee/mem.c`, from `build.h`), `PROOT_UNBUNDLE_LOADER`
   (`execve/enter.c:471,565`), `USERLAND` (`fake_id0.c`), `WITH_LIBANDROID_SHMEM`
   (`sysvipc.c:166`), `HAS_POKEDATA_WORKAROUND` (aarch64, `arch.h:151`) are all
   conditional. Whether the Termux package defines `USERLAND` or
   `PROOT_UNBUNDLE_LOADER` was not determined from this source tree alone.
   *Would settle it:* the Termux `proot` build recipe / `Android.mk`.
7. **UNVERIFIED — `sysnums-*.h` table contents.** `sysnum.c` maps neutral
   `Sysnum` ↔ arch numbers via `SYSNUMS_HEADER*` (`arch.h:58-64, 90, 116-120`);
   the tables were not enumerated.
8. **UNVERIFIED — how the embedded loader is produced.** `execve/enter.c:472-476`
   declares `_binary_loader_exe_start/_end` and a WEAK `_binary_loader_m32_*`.
   The extraction is read; the build rule that embeds them was not.
9. **UNVERIFIED — `loader/assembly-{arm,x86,x86_64}.h`.** Only the arm64 header
   was read; the others are assumed to define the same `BRANCH`,
   `PREPARE_ARGS_*`, `OUTPUT_CONTRAINTS_*`, `SYSCALL` and syscall numbers.
10. **Not verified here (out of tree):** whether `proot-distro` (or PDN's argv
    builder) passes `-0`, `--link2symlink`, `--sysvipc`, `--kill-on-exit` by
    default. In *proot itself* none of these are default (§4).

---

## 6. Claim → evidence table

Commands are labelled C1..C14; the exact text run is given so a reader can
re-run them. All were run inside
`/data/data/com.termux/files/home/.hermes/cache/scratch/proot-src`.

| # | Claim | Evidence |
|---|---|---|
| 1 | Source commit is `52d65a4dc140ca554d241a1438ddd4f6b00c9813` | C1: `git log -1 --format=%H` → `52d65a4dc140ca554d241a1438ddd4f6b00c9813` |
| 2 | In-tree CLI version is `5.1.0` | `src/cli/proot.h:9` (`#define VERSION "5.1.0"`) |
| 3 | Scoped dirs total 24,284 lines | C2: `wc -l src/syscall/*.c src/tracee/*.c src/execve/*.c src/loader/*.c src/extension/extension.c src/extension/*/*.c src/cli/cli.c src/cli/proot.c` → `24284 total` |
| 4 | Individual line counts (enter.c 3069, exit.c 845, socket.c 622, seccomp.c 583, tracee.c 1053, event.c 962, mem.c 692, tracee/seccomp.c 673, execve/enter.c 731, execve/exit.c 513, ldso.c 579, loader.c 263, extension.c 169, kompat.c 1049, fake_id0.c 1543, sysvipc.c 370, ashmem_memfd.c 242, link2symlink.c 1788, fix_symlink_size.c 119, hidden_files.c 196, port_switch.c 256, mountinfo.c 208) | C3: `wc -l src/syscall/*.c src/tracee/*.c src/execve/*.c src/loader/*.c src/extension/*.c src/*.c \| sort -n` |
| 5 | Only 9 `initialize_extension()` call sites, all in `src/cli/`; `mountinfo` is the only one not behind an option | C4: `grep -rn "initialize_extension" src/cli/` (output reproduced in §4) |
| 6 | `mountinfo` default ON unless `PROOT_NO_MOUNTINFO` | `src/cli/cli.c:481-482`; C4 shows `cli.c:482`; grep C5 confirms `PROOT_NO_MOUNTINFO` only at `cli.c:481` |
| 7 | `PROOT_NO_SECCOMP` gates the seccomp filter | C5: `grep -rn "PROOT_NO_SECCOMP" src/` → `tracee/event.c:145`, `cli/cli.c:140,143`, and a comment in `link2symlink.c:1189` |
| 8 | `PROOT_NO_SECCOMP` also gates a diagnostic, not the filter build | `src/cli/cli.c:140-143` |
| 9 | 108 syscalls in the default seccomp filter | C6: `awk '/static FilteredSysnum proot_sysnums/,/FILTERED_SYSNUM_END/' src/syscall/seccomp.c \| grep -c "PR_"` → `108`; full list at `src/syscall/seccomp.c:331-445` |
| 10 | `enable_syscall_filtering()` is called only in the launch child, before the first execve | `src/tracee/event.c:145-146` |
| 11 | Enter-error path voids the syscall (sets `PR_void`) and pokes the error | `src/syscall/syscall.c:191-198` |
| 12 | `PR_void` → `SYSCALL_AVOIDER`; value is `-2` (default), `222` (ARM EABI), `-1` (ARM64) | `src/syscall/sysnum.c:110-111`; `src/arch.h:34,112,160` |
| 13 | `getcwd` is voided at enter and rebuilt at exit from the emulated cwd | `src/syscall/enter.c:1950-1954`; `src/syscall/exit.c:95-128` |
| 14 | `execveat` with non-`AT_FDCWD` fd returns `-ENOSYS` | `src/syscall/enter.c:1922-1934` |
| 15 | `mount`/`pivot_root`/`umount`/`unshare`/`setns` are emulated and voided | `src/syscall/enter.c:2510-2530`, `2586-2598`; emulation bodies `enter.c:292-331`, `340-473`, `483-506` |
| 16 | `fsopen`-family forced `-ENOSYS` | `src/syscall/enter.c:2607-2615` |
| 17 | `clone`/`clone3` strip `CLONE_NEW*` and force the exit stage | `src/syscall/enter.c:2542-2581` |
| 18 | `/dev/shm` is faked as tmpfs (`f_type = 0x01021994`) on `statfs` exit and on SIGSYS | `src/syscall/exit.c:744-759`; `src/tracee/seccomp.c:352-358` |
| 19 | `brk` is rewritten to `mmap`/`mremap` | `src/syscall/heap.c:73-145`, `137-142`, `150-210` |
| 20 | Chained syscalls: FIFO, args poked, IP moved back, `PTRACE_SYSCALL` | `src/syscall/chain.c:87-96`, `133-152` |
| 21 | Memory access prefers `process_vm_*` with a ptrace fallback; `alloc_mem` is enter-only and pushes below SP | `src/tracee/mem.c:218-291`, `338-403`, `637-673` |
| 22 | `PTRACE_POKEDATA` workaround exists for aarch64 (via tracee stub); unsupported on aarch32 | `src/tracee/mem.c:98-197`; `src/arch.h:151` |
| 23 | Seccomp chooses `PTRACE_CONT` when no exit stage is needed | `src/tracee/event.c:488-499` |
| 24 | Seccomp syscall-order is auto-detected and logged | `src/tracee/event.c:691-697`; forced by `PROOT_ASSUME_NEW_SECCOMP` at `event.c:477` |
| 25 | Outer-policy SIGSYS is caught and legacy syscalls rewritten / emulated | `src/tracee/event.c:837-891`; `src/tracee/seccomp.c:85-133`, `148-672` |
| 26 | `statx` is emulated with host `stat`/`lstat`/`fstat` | `src/tracee/statx.c:7-146`; hooked at `src/syscall/exit.c:768-770` and `src/tracee/seccomp.c:601-606` |
| 27 | `execve` runs proot's embedded loader instead of the program | `src/execve/enter.c:718-728` |
| 28 | Loader is extracted from embedded symbols to a temp file, `u+rx`, path via `/proc/self/fd`; overridable by `PROOT_LOADER`/`PROOT_LOADER_32` | `src/execve/enter.c:483-556`, `563-589` |
| 29 | Load script is serialized from `load_info->mappings` and pushed below SP, `USERARG_1` repointed | `src/execve/exit.c:144-168`, `174-403`, `362-366` |
| 30 | Loader interprets `LoadStatement`s, patches auxv, then `BRANCH`/`EXECVE` | `src/loader/loader.c:111-263`; layout `src/loader/script.h:29-76` |
| 31 | ARM64 loader branch is inline asm (`mov sp`, `mov x0,#0`, `br`) | `src/loader/assembly-arm64.h:30-44` |
| 32 | `AT_EXECFN` is fixed by the loader and additionally by proot for `PR_GET_AUXV` / `/proc/self/auxv` read | `src/execve/exit.c:208-214`; `src/syscall/exit.c:586-622`, `624-687`; `src/tracee/tracee.h:302-311` |
| 33 | Shebang scripts are expanded into `interpreter [arg] script` argv | `src/execve/shebang.c:208-307`, `266-293` |
| 34 | Mixed-mode (QEMU) rebuilds `LD_LIBRARY_PATH` | `src/execve/ldso.c:451-579`; `ldso_env_passthru` `ldso.c:100-177` |
| 35 | `link2symlink` only via `-l`/`--link2symlink` | `src/cli/proot.c:284-294`; `src/cli/proot.h:237-246` |
| 36 | `sysvipc` only via `--sysvipc`; spawns `--shm-helper` re-exec | `src/cli/proot.c:310-320`; `src/cli/proot.h:247-256`; `src/cli/cli.c:457-459`; `src/extension/sysvipc/sysvipc_shm.c:156` |
| 37 | `ashmem_memfd` only via `--ashmem-memfd`, Android-only build; probes memfd via fork, override env | `src/cli/proot.c:296-308`; `src/cli/proot.h:257-266`; `src/extension/ashmem_memfd/ashmem_memfd.c:1,39-86,172-240` |
| 38 | `fix_symlink_size`/`hidden_files`/`port_switch` only via `-L`/`-H`/`-p` | `src/cli/proot.c:322-338`; `src/cli/proot.h:267-290` |
| 39 | `kompat` only via `-k`/`--kernel-release` | `src/cli/proot.c:156-173`; `src/cli/proot.h:196-208` |
| 40 | `fake_id0` only via `-0`/`--root-id`/`-i`/`--change-id`/`-S`; `-0` == `-i 0:0`; `-S` == `-0 -r -b...` | `src/cli/proot.c:175-193`, `267-282`; `src/cli/proot.h:209-235`, `324-344` |
| 41 | `kill-on-exit` is opt-in via `--kill-on-exit` → `tracee->killall_on_exit` | `src/cli/proot.c:195-199`; `src/cli/proot.h:155-164`; consumed at `src/tracee/tracee.c:356-361`, `event.c:317` |
| 42 | Extension inheritance is parent-decided (`INHERIT_PARENT` <0/0/>0) | `src/extension/extension.c:128-168`; semantics `src/extension/extension.h:98-113` |
| 43 | Launch child resets SIGPIPE to `SIG_DFL`, `PTRACE_TRACEME`s, then `SIGSTOP`s before exec | `src/tracee/event.c:126-141` |
| 44 | `--shm-helper` is handled before anything else in `main` | `src/cli/cli.c:457-459` |
| 45 | `-q` additionally binds `/` to `/host-rootfs` and `/dev/null` over `/etc/ld.so.preload` | `src/cli/proot.c:141-142` |

### Reproduction commands (verbatim)

```
C1  git -C /data/data/com.termux/files/home/.hermes/cache/scratch/proot-src log -1 --format=%H
C2  cd /data/data/com.termux/files/home/.hermes/cache/scratch/proot-src && wc -l src/syscall/*.c src/tracee/*.c src/execve/*.c src/loader/*.c src/extension/extension.c src/extension/*/*.c src/cli/cli.c src/cli/proot.c
C3  cd /data/data/com.termux/files/home/.hermes/cache/scratch/proot-src && wc -l src/syscall/*.c src/tracee/*.c src/execve/*.c src/loader/*.c src/extension/*.c src/*.c | sort -n
C4  cd /data/data/com.termux/files/home/.hermes/cache/scratch/proot-src && grep -rn "initialize_extension" src/cli/
C5  cd /data/data/com.termux/files/home/.hermes/cache/scratch/proot-src && grep -rn "PROOT_NO_SECCOMP\|PROOT_NO_MOUNTINFO" src/cli/ src/tracee/
C6  cd /data/data/com.termux/files/home/.hermes/cache/scratch/proot-src && awk '/static FilteredSysnum proot_sysnums/,/FILTERED_SYSNUM_END/' src/syscall/seccomp.c | grep -c "PR_"
C7  cd /data/data/com.termux/files/home/.hermes/cache/scratch/proot-src && grep -rn "link2symlink|sysvipc|ashmem|fix_symlink_size|hidden_files|port_switch|kompat|fake_id0|kill-on-exit" src/cli/
C8  cd /data/data/com.termux/files/home/.hermes/cache/scratch/proot-src && grep -rn "shm-helper\|shm_helper\|sysvipc_shm_helper_main" src/
C9  cd /data/data/com.termux/files/home/.hermes/cache/scratch/proot-src && grep -rn "PROOT_NO_MOUNTINFO\|PROOT_NO_SECCOMP\|PROOT_DONT_SHARE_LIBANDROID_SHMEM\|PROOT_ASSUME_MEMFD" src/
C10 cd /data/data/com.termux/files/home/.hermes/cache/scratch/proot-src && grep -rn "case INITIALIZATION\|INHERIT_PARENT\|link2symlink_callback\|extension->filtered_sysnums" src/extension/link2symlink/link2symlink.c
C11 show_large_files: src/syscall/enter.c 3069 lines; read in 3 windows (1-1050, 1051-2100, 2101-3069)
C12 read_file src/tracee/tracee.h (437 lines) — RegVersion enum, struct fields, HOST_ROOTFS
C13 read_file src/arch.h (206 lines) — SYSCALL_AVOIDER, PIC/LOADER addresses, seccomp arch map
C14 read_file src/extension/extension.{c,h} — registration, events, inheritance
```

---

*End of dissection. Read-only: the proot source tree was not modified. The only
file written is this one. Temp files, if any, were confined to
`$PREFIX/tmp`.*
