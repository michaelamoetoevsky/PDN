# Security

PDN implements a defense-in-depth security model with multiple layers of protection.

## Security Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        SECURITY LAYERS                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ 1. SECCOMP FILTERING (Kernel)                           │    │
│  │    └─ System call filtering, argument inspection        │    │
│  └─────────────────────────────────────────────────────────┘    │
│                              │                                   │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ 2. LINUX CAPABILITIES (Kernel)                          │    │
│  │    └─ Drop-by-default, minimal privilege                │    │
│  └─────────────────────────────────────────────────────────┘    │
│                              │                                   │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ 3. APPARMOR / SELINUX (LSM)                             │    │
│  │    └─ Mandatory access control, path-based policies     │    │
│  └─────────────────────────────────────────────────────────┘    │
│                              │                                   │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ 4. NAMESPACES (Kernel)                                  │    │
│  │    └─ PID, NET, IPC, UTS, CGROUP, USER, MOUNT           │    │
│  └─────────────────────────────────────────────────────────┘    │
│                              │                                   │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ 5. CGROUPS V2 (Kernel)                                  │    │
│  │    └─ Resource limits: CPU, memory, IO, PIDs            │    │
│  └─────────────────────────────────────────────────────────┘    │
│                              │                                   │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ 6. FILESYSTEM ISOLATION (PRoot + proot-compat)          │    │
│  │    └─ Read-only rootfs, masked paths, no-new-privs      │    │
│  └─────────────────────────────────────────────────────────┘    │
│                              │                                   │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ 7. GATEWAY GUARDRAILS (User-space)                      │    │
│  │    └─ Policy enforcement, rate limiting, quotas         │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Seccomp Profiles

### Default Profile

Balanced security for general workloads. Allows ~200 syscalls, blocks dangerous ones.

```toml
[profile.seccomp]
default_action = "errno"
architectures = ["SCMP_ARCH_AARCH64", "SCMP_ARCH_X86_64", "SCMP_ARCH_X86", "SCMP_ARCH_X32"]

# Blocked syscalls (action = "kill")
blocked = [
    "ptrace", "process_vm_readv", "process_vm_writev",
    "userfaultfd", "bpf", "kcmp", "finit_module",
    "delete_module", "init_module", "reboot",
    "kexec_load", "kexec_file_load", "modify_ldt",
    "vm86", "ioperm", "iopl", "perf_event_open"
]
```

### Restricted Profile

Minimal syscall set for untrusted workloads. Only ~50 syscalls allowed.

```toml
[profile.seccomp]
default_action = "kill"
architectures = ["SCMP_ARCH_AARCH64", "SCMP_ARCH_X86_64"]

# Only essential syscalls allowed
allowed = [
    "read", "write", "open", "close", "stat", "fstat", "lstat",
    "poll", "select", "epoll_wait", "epoll_ctl", "epoll_pwait",
    "mmap", "munmap", "brk", "mprotect", "madvise",
    "rt_sigaction", "rt_sigprocmask", "rt_sigreturn", "sigaltstack",
    "getpid", "getppid", "getuid", "geteuid", "getgid", "getegid",
    "gettid", "getpid", "getrandom", "clock_gettime", "nanosleep",
    "futex", "set_tid_address", "exit", "exit_group", "wait4",
    "clone", "unshare", "setns", "sched_yield", "sched_getaffinity",
    "sched_setaffinity", "getrlimit", "setrlimit", "prlimit64",
    "prctl", "arch_prctl", "personality", "uname", "sysinfo",
    "ioctl", "fcntl", "dup", "dup2", "dup3", "pipe", "pipe2",
    "socket", "connect", "accept", "bind", "listen", "sendto",
    "recvfrom", "sendmsg", "recvmsg", "shutdown", "getsockopt",
    "setsockopt", "getpeername", "getsockname"
]
```

### Unconfined Profile

No seccomp filtering - for debugging only.

```toml
[profile.seccomp]
default_action = "allow"
architectures = ["SCMP_ARCH_AARCH64", "SCMP_ARCH_X86_64"]
```

### Custom Profiles

Create custom profiles in `~/.config/pn-next/security/<name>.toml`:

```toml
[profile]
name = "my-custom"
type = "custom"
enabled = true

[profile.seccomp]
default_action = "errno"
architectures = ["SCMP_ARCH_AARCH64", "SCMP_ARCH_X86_64"]

[profile.seccomp.rules]
# Allow specific syscalls with argument filtering
"clone" = { action = "allow", args = [
    { index = 0, value = 0x00000100, op = "masked_equal" }  # CLONE_NEWUSER
]}

"openat" = { action = "allow", args = [
    { index = 1, value = 0, op = "masked_equal", value_two = 0x00001000 }  # O_RDONLY
]}

# Block specific syscalls
"ptrace" = { action = "kill" }
"bpf" = { action = "kill" }
```

### Seccomp Argument Filtering

```toml
[profile.seccomp.rules]
# Filter by syscall argument
"openat" = { action = "allow", args = [
    { index = 1, value = 0, op = "masked_equal", value_two = 0x00001000 },  # O_RDONLY only
    { index = 2, value = 0, op = "masked_equal", value_two = 0 }  # mode = 0
]}

# Filter by architecture
"arch_prctl" = { action = "allow", architectures = ["SCMP_ARCH_X86_64"] }

# Complex rules with multiple conditions
"mmap" = { action = "allow", args = [
    { index = 3, value = 0x00000002, op = "masked_equal", value_two = 0x00000020 }  # MAP_PRIVATE | MAP_ANONYMOUS
]}
```

### Using Profiles

```bash
# Use default profile (default)
podnext run ubuntu -- cmd

# Use restricted profile
podnext run ubuntu --security-profile restricted -- cmd

# Use unconfined profile (debugging)
podnext run ubuntu --security-profile unconfined -- cmd

# Use custom profile
podnext run ubuntu --security-profile my-custom -- cmd

# Set default profile globally
export PN_SECURITY_PROFILE=restricted
```

## Linux Capabilities

### Capability Model

PDN uses a **drop-by-default** model:

1. Start with all capabilities
2. Drop dangerous capabilities (default profile drops 30+)
3. Add only required capabilities

### Default Dropped Capabilities

| Capability | Description | Reason |
|------------|-------------|--------|
| `CAP_SYS_ADMIN` | System administration | Too powerful |
| `CAP_SYS_MODULE` | Load kernel modules | Kernel compromise |
| `CAP_SYS_RAWIO` | Raw I/O access | Hardware access |
| `CAP_DAC_OVERRIDE` | Bypass file permissions | File access control |
| `CAP_SYS_PTRACE` | Process tracing | Info leakage |
| `CAP_SYS_RESOURCE` | Resource limits override | DoS |
| `CAP_SYS_BOOT` | Reboot system | Availability |
| `CAP_SYS_NICE` | Change priority | DoS |
| `CAP_SYS_TIME` | Set system time | Integrity |
| `CAP_MKNOD` | Create device nodes | Hardware access |
| `CAP_AUDIT_WRITE` | Write audit log | Info leakage |
| `CAP_AUDIT_CONTROL` | Control audit | Audit bypass |
| `CAP_MAC_OVERRIDE` | Override MAC | MAC bypass |
| `CAP_MAC_ADMIN` | MAC administration | MAC bypass |
| `CAP_SYSLOG` | Read kernel log | Info leakage |
| `CAP_NET_ADMIN` | Network administration | Network control |
| `CAP_NET_RAW` | Raw sockets | Network attacks |
| `CAP_SYS_CHROOT` | chroot() | Escape risk |
| `CAP_SYS_PACCT` | Process accounting | Info leakage |
| `CAP_WAKE_ALARM` | Wake alarms | DoS |
| `CAP_BLOCK_SUSPEND` | Block suspend | DoS |

### Default Added Capabilities

| Capability | Description | Reason |
|------------|-------------|--------|
| `CAP_CHOWN` | Change file ownership | Basic file ops |
| `CAP_FSETID` | Set setuid/setgid | Executable execution |
| `CAP_KILL` | Send signals | Process management |
| `CAP_SETGID` | Set group ID | User switching |
| `CAP_SETUID` | Set user ID | User switching |
| `CAP_SETPCAP` | Modify capabilities | Capability management |
| `CAP_NET_BIND_SERVICE` | Bind to ports <1024 | Web servers |

### Customizing Capabilities

```toml
[container.config.capabilities]
drop = [
    "CAP_SYS_ADMIN", "CAP_SYS_MODULE", "CAP_SYS_RAWIO"
]
add = [
    "CAP_NET_ADMIN", "CAP_DAC_OVERRIDE"
]
```

```bash
# Runtime override
podnext run ubuntu --cap-add NET_ADMIN --cap-drop SYS_ADMIN -- cmd
```

## AppArmor

### Default Profile

```apparmor
# /etc/apparmor.d/pn-next-default
#include <tunables/global>

profile pn-next-default flags=(attach_disconnected,mediate_deleted) {
    #include <abstractions/base>
    #include <abstractions/nameservice>

    capability setuid,
    capability setgid,
    capability chown,
    capability fsetid,
    capability kill,
    capability setpcap,
    capability net_bind_service,

    network inet stream,
    network inet6 stream,
    network netlink route,

    file,
    deny /proc/** w,
    deny /sys/** w,
    deny /sys/firmware/** r,
    deny /sys/kernel/security/** r,

    mount,
    umount,

    /bin/** ix,
    /usr/bin/** ix,
    /usr/local/bin/** ix,
    /lib/** mr,
    /usr/lib/** mr,
    /usr/local/lib/** mr,

    /etc/** r,
    /var/** rw,
    /tmp/** rw,
    /run/** rw,
    /home/** rw,
    /root/** rw,
    /mnt/** rw,
    /media/** rw,
    /opt/** rw,
    /srv/** rw,
}
```

### Loading Profiles

```bash
# Load profile
sudo apparmor_parser -r /etc/apparmor.d/pn-next-default

# Check status
sudo aa-status | grep pn-next

# Set to complain mode (for debugging)
sudo aa-complain /etc/apparmor.d/pn-next-default

# Set to enforce mode
sudo aa-enforce /etc/apparmor.d/pn-next-default
```

### Custom Profiles

```bash
# Create custom profile
cat > /etc/apparmor.d/pn-next-myapp << 'EOF'
#include <tunables/global>

profile pn-next-myapp flags=(attach_disconnected) {
    #include <abstractions/base>
    
    # App-specific rules
    /usr/bin/myapp ix,
    /var/lib/myapp/** rw,
    /etc/myapp/** r,
    
    # Network
    network inet stream,
    network inet6 stream,
    
    # Capabilities
    capability setuid,
    capability setgid,
}
EOF

# Load and use
sudo apparmor_parser -r /etc/apparmor.d/pn-next-myapp
podnext run myapp --security-profile myapp -- cmd
```

## SELinux

### Default Context

```bash
# Default container context
system_u:system_r:container_t:s0:c123,c456

# Check SELinux status
sestatus

# Check container contexts
ps -eZ | grep container_t
```

### Custom Policies

```bash
# Create custom policy module
cat > pn-next-myapp.te << 'EOF'
module pn-next-myapp 1.0;

require {
    type container_t;
    type myapp_exec_t;
    class process { transition dyntransition };
    class file { execute read getattr };
}

# Allow transition to custom domain
type myapp_t;
type myapp_exec_t;
init_daemon_domain(myapp_t)
domain_entry_file(myapp_t, myapp_exec_t)

# Allow network access
allow myapp_t self:tcp_socket { create connect };
allow myapp_t self:udp_socket { create connect };

# Allow file access
allow myapp_t myapp_var_lib_t:dir { read write search };
allow myapp_t myapp_var_lib_t:file { read write create unlink };
EOF

# Compile and load
make -f /usr/share/selinux/devel/Makefile pn-next-myapp.pp
semodule -i pn-next-myapp.pp

# Label executable
semanage fcontext -a -t myapp_exec_t "/usr/bin/myapp"
restorecon -v /usr/bin/myapp

# Use with container
podnext run myapp --security-profile myapp -- cmd
```

## Filesystem Isolation

### Read-Only Rootfs

```toml
[container.security]
read_only_rootfs = true
```

```bash
podnext run ubuntu --read-only -- cmd
```

### Masked Paths

Paths hidden from container:

```toml
[container.security.mask_paths]
paths = [
    "/proc/asound", "/proc/bus", "/proc/fs", "/proc/irq",
    "/proc/sys", "/proc/sysrq-trigger", "/proc/acpi",
    "/proc/kcore", "/proc/keys", "/proc/latency_stats",
    "/proc/sched_debug", "/proc/scsi", "/proc/timer_list",
    "/proc/timer_stats", "/sys/firmware", "/sys/kernel/security"
]
```

### Read-Only Paths

Paths visible but not writable:

```toml
[container.security.readonly_paths]
paths = [
    "/proc/bus", "/proc/fs", "/proc/irq", "/proc/sys",
    "/proc/sysrq-trigger", "/sys/firmware", "/sys/kernel/security"
]
```

### No New Privileges

Prevents privilege escalation:

```toml
[container.security]
no_new_privileges = true
```

```bash
podnext run ubuntu --no-new-privileges -- cmd
```

### User Namespaces

Map container UID/GID to unprivileged host UID/GID:

```toml
[container.security]
user_namespace = true
uid_map = "0:100000:65536"
gid_map = "0:100000:65536"
```

## Resource Limits (Cgroups v2)

### Memory Limits

```toml
[container.resources]
memory_limit = "2G"           # Hard limit
memory_swap_limit = "4G"      # Memory + swap
memory_reservation = "1G"     # Soft limit (reclaim trigger)
memory_swappiness = 60        # Swap aggressiveness (0-100)
```

### CPU Limits

```toml
[container.resources]
cpu_limit = "2.0"             # CPUs (2.0 = 2 CPUs)
cpu_shares = 1024             # Relative weight (1024 = default)
cpu_quota = 200000            # Microseconds per period
cpu_period = 100000           # Period in microseconds
cpuset_cpus = "0-1"           # Specific CPUs
cpuset_mems = "0"             # Specific memory nodes
```

### IO Limits

```toml
[container.resources]
io_weight = 100               # IO weight (10-1000)
io_max_read_bps = "100MB/s"   # Read bandwidth limit
io_max_write_bps = "100MB/s"  # Write bandwidth limit
io_max_read_iops = 1000       # Read IOPS limit
io_max_write_iops = 1000      # Write IOPS limit
```

### PIDs Limit

```toml
[container.resources]
pids_limit = 100              # Max processes/threads
```

## Gateway Guardrails

### Policy Enforcement

```toml
[gateway.guardrails]
max_containers = 50
max_memory_total = "16G"
max_cpu_total = "8.0"
max_containers_per_user = 10
allowed_images = ["docker.io/library/*", "ghcr.io/myorg/*"]
blocked_images = ["docker.io/library/bad-image"]
required_security_profile = "default"
blocked_syscalls = ["ptrace", "bpf", "userfaultfd"]
```

### Rate Limiting

```toml
[gateway.guardrails.rate_limit]
requests_per_minute = 100
burst = 20
```

### Quotas

```toml
[gateway.guardrails.quotas]
cpu_per_user = "4.0"
memory_per_user = "8G"
containers_per_user = 10
volumes_per_user = 20
networks_per_user = 5
```

## Image Signing & Verification

### Cosign Integration

```bash
# Sign image
cosign sign --key cosign.key docker.io/myorg/myimage:tag

# Verify image
cosign verify --key cosign.pub docker.io/myorg/myimage:tag

# PDN verifies on pull
podnext install docker.io/myorg/myimage:tag --verify-signature
```

### Notary Integration

```bash
# Initialize notary
notary init docker.io/myorg/myimage

# Add delegation
notary delegation add docker.io/myorg/myimage targets/releases --key delegation.key

# Publish
notary publish docker.io/myorg/myimage

# PDN verifies
podnext install docker.io/myorg/myimage:tag --verify-notary
```

## Security Best Practices

### 1. Use Restricted Profiles for Untrusted Workloads

```bash
podnext run untrusted-app --security-profile restricted -- cmd
```

### 2. Enable Read-Only Rootfs

```toml
[container.security]
read_only_rootfs = true
```

### 3. Drop All Unnecessary Capabilities

```toml
[container.config.capabilities]
drop = [
    "CAP_SYS_ADMIN", "CAP_SYS_MODULE", "CAP_SYS_RAWIO",
    "CAP_DAC_OVERRIDE", "CAP_SYS_PTRACE", "CAP_SYS_RESOURCE",
    "CAP_NET_ADMIN", "CAP_NET_RAW", "CAP_SYS_CHROOT"
]
add = []
```

### 4. Use User Namespaces

```toml
[container.security]
user_namespace = true
```

### 5. Limit Resources

```toml
[container.resources]
memory_limit = "1G"
cpu_limit = "1.0"
pids_limit = 50
io_max_read_bps = "50MB/s"
io_max_write_bps = "50MB/s"
```

### 6. Network Segmentation

```toml
[container.network]
mode = "bridge"
networks = ["isolated-network"]
```

### 7. Enable Image Verification

```bash
podnext install myimage:tag --verify-signature
```

### 8. Regular Security Audits

```bash
# Check for vulnerabilities
cargo audit --workspace

# Check for unsafe code
cargo geiger --workspace

# Check dependencies
cargo deny check advisories bans licenses sources
```

### 9. Monitor Security Events

```bash
# Check audit logs
sudo ausearch -m AVC -ts recent

# Check seccomp violations
dmesg | grep seccomp

# Check AppArmor violations
sudo aa-logprof
```

### 10. Keep Updated

```bash
# Update PDN
cargo install --path crates/podnext --force

# Update dependencies
cargo update

# Check for security advisories
cargo audit
```

## Troubleshooting Security Issues

### Container Fails to Start

```bash
# Try unconfined profile
podnext run ubuntu --security-profile unconfined -- /bin/bash

# Check seccomp logs
dmesg -T | grep -i seccomp

# Check capability errors
dmesg -T | grep -i capability
```

### Permission Denied

```bash
# Check AppArmor
sudo aa-status | grep pn-next

# Check SELinux
sudo ausearch -m AVC -ts recent

# Check file permissions
ls -la /path/to/file
```

### Network Not Working

```bash
# Check CNI
ls /etc/cni/net.d/
cat /etc/cni/net.d/10-pdn-bridge.conf

# Check firewall
sudo nft list ruleset
sudo iptables -L -n
```

### GPU Not Working

```bash
# Check devices
ls -la /dev/dri/

# Check permissions
podnext run ubuntu --gpu --security-profile unconfined -- glxinfo
```

## See Also

- [Configuration](configuration.md) - Security configuration details
- [CLI Reference](cli-reference.md) - Security-related commands
- [Architecture](architecture.md) - Security architecture
- [Migration Guide](migration-guide.md) - Security migration