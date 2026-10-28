# PDN Next Steps - 2026-10-29

## Completed This Session
- ✅ CAP_SYS_ADMIN architecture with safe fallback defaults
- ✅ SIGSYS fixes (disabled prctl in seccomp_bypass)
- ✅ All workspace tests passing (26 tests)
- ✅ podnext CLI simplified, wrapping proot-distro
- ✅ Release binary builds successfully

## Phase 1: Search Command (COMPLETED)
- [x] Add `podnext search <query>` - wraps `proot-distro search`
- [x] Add `--limit` flag
- [x] Test with actual Docker Hub search

## Phase 2: Session Management (COMPLETED)
- [x] `podnext sessions` - better formatted session list
- [x] `podnext attach <container>` - attach to container (creates new session)
- [x] `podnext detach` - documentation (built into shell via Ctrl+D)

## Phase 3: Window/Pane Abstraction (Later)
- [ ] Requires pty management architecture
- [ ] Could integrate with tmux or implement minimal multiplexer
- [ ] Lower priority

## Phase 4: Additional proot-distro Features
- [ ] `podnext backup` / `restore`
- [ ] `podnext copy` / `sync` 
- [ ] `podnext build` / `push`
- [ ] `podnext network` commands
- [ ] `podnext volume` commands

## Phase 5: Integration Testing
- [ ] Test with actual Termux/proot-distro containers
- [ ] Android cross-compile test (NDK r27c)
- [ ] GitHub remote + CI/CD activation

## Notes
- Current CLI wraps proot-distro directly for simplicity
- proot-distro has: search, install, login, run, list, ps, kill, remove, backup, restore, copy, sync, build, push
- We expose subset with simpler UX
- tmux-like sessions would need: session persistence, pty management, reattach capability