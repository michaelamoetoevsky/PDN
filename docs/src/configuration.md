# Configuration

## Configuration Hierarchy

PDN uses a layered configuration system:

1. **Global Config** (`~/.config/pn-next/config.toml`) - System-wide settings
2. **Container Config** (`~/.config/pn-next/containers/<name>.toml`) - Per-container settings
3. **Security Profiles** (`~/.config/pn-next/security/<name>.toml`) - Security hardening
4. **Network Config** (`~/.config/pn-next/networks/<name>.toml`) - Network definitions
5. **Storage Config** (`~/.config/pn-next/storage/<name>.toml`) - Storage pools
6. **CLI Flags** - Override any setting at runtime

## Global Configuration

### `~/.config/pn-next/config.toml`

```toml
# PDN Global Configuration

[general]
# planned: data_dir, cache_dir, config_dir, log_dir, log_level, log_format, log_file, log_rotation, log_max_files, gateway_addr, gateway_timeout, gateway_retry_attempts, default_runtime, default_security_profile, default_memory_limit, default_cpu_limit, default_pids_limit, default_network, default_dns, default_dns_search, default_storage_driver, default_volume_driver, default_registry, insecure_registries, image_pull_policy, enable_criu, enable_gpu, enable_plugins, enable_metrics, enable_tracing
# (no `GeneralConfig` struct exists in any crate; the global config.toml has no loader)
# Data directories
data_dir = "~/.local/share/pn-next"
cache_dir = "~/.cache/pn-next"
config_dir = "~/.config/pn-next"
log_dir = "~/.local/state/pn-next/logs"

# Logging
log_level = "info"          # trace, debug, info, warn, error
log_format = "json"         # json, text, pretty
log_file = "pdn.log"
log_rotation = "daily"
log_max_files = 7

# Gateway
gateway_addr = "unix:///run/pn-gateway.sock"
gateway_timeout = "30s"
gateway_retry_attempts = 3

# Runtime
default_runtime = "auto"    # auto, proot, runc, crun
default_security_profile = "default"

# Resource defaults
default_memory_limit = "2G"
default_cpu_limit = "2.0"
default_pids_limit = 100

# Network
default_network = "bridge"
default_dns = ["1.1.1.1", "8.8.8.8"]
default_dns_search = ["local"]

# Storage
default_storage_driver = "local"
default_volume_driver = "local"

# Image
default_registry = "docker.io"
insecure_registries = []
image_pull_policy = "missing"  # always, missing, never

# Feature flags
enable_criu = true
enable_gpu = true
enable_plugins = true
enable_metrics = true
enable_tracing = false

[gateway]
# NOTE (issue #27 + doc-drift round 2): the global config.toml shown in this example
# is not yet read by any crate (`pn-gateway` has no toml loader), and only the
# [gateway.fail_safe] fields below mirror a real struct
# (`crates/pn-gateway/src/config.rs`). Every key marked `# planned:` in this document
# has no backing field and would be rejected by a deny-unknown-fields loader.
# See research/DOCS_DRIFT_ROUND2.md.
#
# Fail-safe configuration
[gateway.fail_safe]
enabled = true
max_retries = 3
retry_delay = "5s"
fallback_actions = ["Retry", "FallbackToHost", "RestartContainer"]
auto_recovery = true
circuit_breaker = { enabled = true, failure_threshold = 5, timeout = "60s", half_open_requests = 3 }
# planned: bulkhead_max_concurrent, bulkhead_queue_size, retry_base_delay, retry_max_delay

# Guardrails
# planned: max_containers, max_memory_total, max_cpu_total, max_containers_per_user, allowed_images, blocked_images, required_security_profile, blocked_syscalls
# (real GuardrailConfig = enabled, rules, strict_mode, audit_log)
[gateway.guardrails]
max_containers = 50
max_memory_total = "16G"
max_cpu_total = "8.0"
max_containers_per_user = 10
allowed_images = []
blocked_images = []
required_security_profile = "default"
blocked_syscalls = [
    "ptrace", "process_vm_readv", "process_vm_writev",
    "userfaultfd", "bpf", "kcmp", "finit_module"
]

# Health checks
[gateway.health]
# planned: healthy_threshold
# (real HealthConfig = enabled, check_interval, checks, unhealthy_threshold, recovery_actions)
enabled = true
check_interval = "30s"
timeout = "10s"
healthy_threshold = 2
unhealthy_threshold = 3

# Metrics
[gateway.metrics]
# planned: port, path
# (real MetricsConfig = enabled, interval, retention, exporters)
enabled = true
interval = "60s"
port = 9090
path = "/metrics"

[grpc]
# planned: addr, max_recv_msg_size, max_send_msg_size, keepalive_time, keepalive_timeout, keepalive_permit_without_stream
# (no gRPC config struct exists in pn-gateway)
# gRPC server configuration
[grpc.server]
addr = "127.0.0.1:50051"
max_recv_msg_size = 4194304
max_send_msg_size = 4194304
keepalive_time = "20s"
keepalive_timeout = "10s"
keepalive_permit_without_stream = true

[grpc.client]
# planned: max_recv_msg_size, max_send_msg_size
timeout = "30s"
max_recv_msg_size = 4194304
max_send_msg_size = 4194304

[api]
# planned: addr, cors_allowed_origins, cors_allowed_methods, cors_allowed_headers, cors_max_age
# (no `[api.server]` struct exists in pn-gateway)
# REST API configuration
[api.server]
enabled = true
addr = "127.0.0.1:8080"
cors_allowed_origins = ["*"]
cors_allowed_methods = ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
cors_allowed_headers = ["*"]
cors_max_age = 3600

[tracing]
# planned: type, endpoint, sampling_ratio
# (no tracing config struct exists in pn-gateway)
# OpenTelemetry tracing
[tracing.exporter]
type = "otlp"           # otlp, jaeger, zipkin, stdout
endpoint = "http://localhost:4317"
sampling_ratio = 0.1

[plugin]
# planned: plugin_dirs, allow_unsigned, max_plugins
# (no plugin-manager config struct exists in pn-gateway)
# Plugin system
[plugin.manager]
enabled = true
plugin_dirs = [
    "~/.config/pn-next/plugins",
    "/usr/lib/pn-next/plugins"
]
allow_unsigned = false
max_plugins = 20
```

## Loading and validating

The gateway reads its settings from the `[gateway]` table of a PDN configuration file. The file
is named explicitly - there is no implicit search path, so a deployment either names its file or
gets no gateway configuration at all:

```bash
podnext config validate ./gateway.toml
PN_GATEWAY_CONFIG=./gateway.toml podnext config validate
```

`podnext config validate` reads the file with the same loader the gateway uses, so an unknown key,
a malformed value or an unreadable duration fails here rather than at startup. A valid file prints
the values it resolves to and exits 0; an invalid one prints the offending key with its line and
column and exits 2.

Keys under `[gateway]` are strict: an unknown key is always an error, because a typo that is
silently ignored becomes a setting that silently does not apply. Sections outside `[gateway]`
(`[general]`, `[grpc]`, `[api]`, ...) belong to other components and are left to them.

## Duration values

Durations are strings with a unit:

```toml
gateway_timeout = "30s"
retry_delay = "500ms"
check_interval = "2m"
```

| unit | meaning |
|---|---|
| `ms` | milliseconds |
| `s` | seconds |
| `m` | minutes |
| `h` | hours |
| `d` | days |

Long forms (`"30 seconds"`, `"2 minutes"`) are accepted as well. A bare number is read as
seconds (`timeout = 30`), and the native `{ secs = 30, nanos = 0 }` table still loads, so
configuration written before the string form existed keeps working.

An unknown unit is a load error that names the offending value, for example
`unknown duration unit "parsecs" in "5 parsecs"`. That is deliberate: falling back to zero
would turn a typo into a disabled timeout.

When PDN writes a configuration back out, durations are rendered in this string form using
the largest exact unit, so `300s` is written as `"5m"`.


## Container Configuration

### `~/.config/pn-next/containers/<name>.toml`

```toml
[container]
name = "ubuntu"
distro = "ubuntu"
version = "22.04"
# planned: created, updated
# (real ContainerConfig uses distro+version, not image; created/updated are metadata)

[container.config]
# planned: domainname, user, group, groups, entrypoint, cmd, stop_signal, stop_timeout
# (the real ContainerConfig is FLAT — there is no [container.config] table; hostname and
#  working_dir are top-level fields; the rest have no backing field)
# Basic settings
hostname = "ubuntu"
domainname = ""
working_dir = "/root"
user = "root"
group = "root"
groups = []

# Environment variables
[container.config.env]
TERM = "xterm-256color"
LANG = "en_US.UTF-8"
LC_ALL = "en_US.UTF-8"
PATH = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
HOME = "/root"
SHELL = "/bin/bash"

# Labels and annotations
[container.config.labels]
"org.pdn.description" = "Ubuntu 22.04 development container"
"org.pdn.maintainer" = "user@example.com"

[container.config.annotations]
"org.opencontainers.image.title" = "Ubuntu 22.04"
"org.opencontainers.image.version" = "22.04"

# Entrypoint and command
entrypoint = ["/bin/bash"]
cmd = ["-l"]

# Stop signal and timeout
stop_signal = "SIGTERM"
stop_timeout = 10

# Mounts
[container.config.mounts]
# Bind mounts: host_path = { target = "guest_path", readonly = false, recursive = true, propagation = "rprivate" }
"/sdcard" = { target = "/mnt/sdcard", readonly = false }
"/data/data/com.termux/files/home" = { target = "/home/termux", readonly = false }
"/tmp" = { target = "/tmp", readonly = false, propagation = "rshared" }

# Volumes
[container.config.volumes]
"my-data" = { target = "/data", readonly = false }
"shared-cache" = { target = "/var/cache", readonly = false }

# Temporary filesystems
[container.config.tmpfs]
"/run" = { size = "100M", mode = "0755" }
"/tmp" = { size = "500M", mode = "1777" }

# Devices
[container.config.devices]
"/dev/dri/renderD128" = { target = "/dev/dri/renderD128", permissions = "rwm" }
"/dev/dri/card0" = { target = "/dev/dri/card0", permissions = "rwm" }

# Sysctls
[container.config.sysctls]
"net.ipv4.ip_forward" = "1"
"kernel.shmmax" = "67108864"

# Ulimits
[container.config.ulimits]
"nofile" = { soft = 65536, hard = 65536 }
"nproc" = { soft = 1024, hard = 2048 }

# Capabilities
[container.config.capabilities]
drop = [
    "CAP_SYS_ADMIN", "CAP_SYS_MODULE", "CAP_SYS_RAWIO",
    "CAP_DAC_OVERRIDE", "CAP_SYS_PTRACE", "CAP_SYS_RESOURCE",
    "CAP_SYS_BOOT", "CAP_SYS_NICE", "CAP_SYS_TIME",
    "CAP_SYS_TTY_CONFIG", "CAP_MKNOD", "CAP_AUDIT_WRITE",
    "CAP_AUDIT_CONTROL", "CAP_MAC_OVERRIDE", "CAP_MAC_ADMIN",
    "CAP_SYSLOG", "CAP_WAKE_ALARM", "CAP_BLOCK_SUSPEND"
]
add = []

[container.config.seccomp]
# planned: custom_rules
seccomp_profile = "default"

[container.config.apparmor]
apparmor_profile = "pn-next-default"
enforce = true

[container.config.selinux]
selinux_label = "system_u:system_r:container_t:s0:c123,c456"
enforce = true

# planned: memory_swap_limit, memory_reservation, cpu_shares, cpu_quota, cpu_period, cpuset_cpus, cpuset_mems, io_weight, io_max_read_bps, io_max_read_iops, io_max_write_bps, io_max_write_iops
# (real ResourceLimits = cpu_limit, memory_limit, disk_limit, pids_limit)
[container.resources]
memory_limit = "2G"
memory_swap_limit = "4G"
memory_reservation = "1G"
cpu_limit = "2.0"
cpu_shares = 1024
cpu_quota = 200000
cpu_period = 100000
cpuset_cpus = "0-1"
cpuset_mems = "0"
pids_limit = 100
io_weight = 100
io_max_read_bps = "100MB/s"
io_max_write_bps = "100MB/s"
io_max_read_iops = 1000
io_max_write_iops = 1000

# planned: mask_paths, readonly_paths
# (real SecurityConfig = privileged, capabilities, seccomp_profile, apparmor_profile,
#  selinux_label, no_new_privileges, read_only_rootfs)
[container.security]
seccomp_profile = "default"
no_new_privileges = true
read_only_rootfs = false
user_namespace = false
mask_paths = [
    "/proc/asound", "/proc/bus", "/proc/fs", "/proc/irq",
    "/proc/sys", "/proc/sysrq-trigger"
]
readonly_paths = [
    "/proc/bus", "/proc/fs", "/proc/irq", "/proc/sys",
    "/proc/sysrq-trigger"
]

# planned: networks, dns_search, dns_options, extra_hosts, ports, ipc_mode, uts_mode
# (real container NetworkConfig = mode, bridge, ip_address, gateway, dns_servers, port_mappings)
[container.network]
mode = "bridge"          # bridge, host, none, container:<name>
networks = ["default"]
dns_servers = ["1.1.1.1", "8.8.8.8"]
dns_search = ["local"]
dns_options = ["ndots:2"]
extra_hosts = { "host.docker.internal" = "host-gateway" }
ports = []
ipc_mode = "shareable"
uts_mode = ""

# planned: api
[container.gpu]
enabled = false
devices = []
api = "vulkan"           # vulkan, opengl, opencl, cuda

# planned: max_checkpoints
[container.checkpoint]
enabled = false
interval = "30m"
max_checkpoints = 5
storage = "local"

# planned: prestart, poststart, prestop, poststop
[container.hooks]
prestart = []
poststart = []
prestop = []
poststop = []
```

## Security Profiles

### `~/.config/pn-next/security/<name>.toml`

```toml
# planned: description
[profile]
name = "default"
profile_type = "default"         # default, restricted, unconfined, custom
enabled = true
description = "Balanced security profile for general use"
no_new_privileges = true
read_only_rootfs = false
user_namespace = false

[profile.seccomp]
default_action = "errno"  # kill, kill_process, trap, errno, trace, allow, log
architectures = [
    "SCMP_ARCH_AARCH64",
    "SCMP_ARCH_X86_64",
    "SCMP_ARCH_X86",
    "SCMP_ARCH_X32"
]

# Syscall rules (overrides default profile)
[profile.seccomp.rules]
# Allow specific syscalls with argument filtering
"clone" = { action = "allow", args = [{ index = 0, value = 0x00000100, op = "masked_equal" }] }  # CLONE_NEWUSER
"unshare" = { action = "allow", args = [{ index = 0, value = 0x10000000, op = "masked_equal" }] }  # CLONE_NEWNS

# Block dangerous syscalls
"ptrace" = { action = "kill" }
"process_vm_readv" = { action = "kill" }
"process_vm_writev" = { action = "kill" }
"userfaultfd" = { action = "kill" }
"bpf" = { action = "kill" }
"kcmp" = { action = "kill" }
"finit_module" = { action = "kill" }
"delete_module" = { action = "kill" }
"init_module" = { action = "kill" }

[profile.capabilities]
# Capabilities to drop (removed from container)
drop = [
    "CAP_SYS_ADMIN", "CAP_SYS_MODULE", "CAP_SYS_RAWIO",
    "CAP_DAC_OVERRIDE", "CAP_SYS_PTRACE", "CAP_SYS_RESOURCE",
    "CAP_SYS_BOOT", "CAP_SYS_NICE", "CAP_SYS_TIME",
    "CAP_SYS_TTY_CONFIG", "CAP_MKNOD", "CAP_AUDIT_WRITE",
    "CAP_AUDIT_CONTROL", "CAP_MAC_OVERRIDE", "CAP_MAC_ADMIN",
    "CAP_SYSLOG", "CAP_WAKE_ALARM", "CAP_BLOCK_SUSPEND",
    "CAP_DAC_READ_SEARCH", "CAP_FOWNER", "CAP_IPC_LOCK",
    "CAP_IPC_OWNER", "CAP_LEASE", "CAP_LINUX_IMMUTABLE",
    "CAP_NET_ADMIN", "CAP_NET_BROADCAST", "CAP_NET_RAW",
    "CAP_SYS_CHROOT", "CAP_SYS_PACCT", "CAP_SYS_RESOURCE",
    "CAP_SYS_TTY_CONFIG"
]
# Capabilities to add (granted to container)
add = [
    "CAP_CHOWN", "CAP_FSETID", "CAP_KILL", "CAP_SETGID",
    "CAP_SETUID", "CAP_SETPCAP", "CAP_NET_BIND_SERVICE"
]

# planned: custom_rules
[profile.apparmor]
profile_name = "pn-next-default"
enforce = true
# Custom AppArmor rules
custom_rules = [
    "network inet stream,",
    "network inet6 stream,",
    "capability setuid,",
    "capability setgid,"
]

# planned: custom_rules
[profile.selinux]
context = "system_u:system_r:container_t:s0:c123,c456"
enforce = true
# Custom SELinux rules
custom_rules = [
    "allow container_t self:capability { setuid setgid };",
    "allow container_t self:netlink_route_socket { nlmsg_read nlmsg_write };"
]

# planned: paths
[profile.mask_paths]
paths = [
    "/proc/asound", "/proc/bus", "/proc/fs", "/proc/irq",
    "/proc/sys", "/proc/sysrq-trigger", "/proc/acpi",
    "/proc/kcore", "/proc/keys", "/proc/latency_stats",
    "/proc/sched_debug", "/proc/scsi", "/proc/timer_list",
    "/proc/timer_stats", "/sys/firmware", "/sys/kernel/security"
]

# planned: paths
[profile.readonly_paths]
paths = [
    "/proc/bus", "/proc/fs", "/proc/irq", "/proc/sys",
    "/proc/sysrq-trigger", "/sys/firmware", "/sys/kernel/security"
]
```

### Predefined Profiles

#### `restricted` - Minimal syscalls, maximum security

```toml
[profile]
name = "restricted"
type = "restricted"
enabled = true
description = "Maximum security - minimal syscalls allowed"
no_new_privileges = true
read_only_rootfs = true
user_namespace = true

[profile.seccomp]
default_action = "kill"
architectures = ["SCMP_ARCH_AARCH64", "SCMP_ARCH_X86_64"]
# Only essential syscalls allowed
rules = {}
```

#### `unconfined` - No restrictions (for debugging)

```toml
[profile]
name = "unconfined"
type = "unconfined"
enabled = true
description = "No security restrictions - for debugging only"
no_new_privileges = false
read_only_rootfs = false
user_namespace = false

[profile.seccomp]
default_action = "allow"
architectures = ["SCMP_ARCH_AARCH64", "SCMP_ARCH_X86_64"]
rules = {}

[profile.capabilities]
drop = []
add = [
    "CAP_SYS_ADMIN", "CAP_SYS_MODULE", "CAP_SYS_RESOURCE",
    "CAP_DAC_OVERRIDE", "CAP_SYS_PTRACE", "CAP_SYS_RAWIO"
]
```

## Network Configuration

### `~/.config/pn-next/networks/<name>.toml`

```toml
# planned: created
# (the persisted network file is a FLAT NetworkConfig: name, driver, subnet, gateway,
#  dns_servers, enable_ipv6, mtu, containers, ips — there is no [network.config] table)
[network]
name = "default"
driver = "bridge"
created = "2024-01-15T10:30:00Z"

# planned: ip_range, ipv6_subnet, ipv6_gateway, dns_search, internal, enable_icc
[network.config]
# Bridge network settings
subnet = "10.88.0.0/16"
gateway = "10.88.0.1"
ip_range = "10.88.0.0/20"
mtu = 1500
enable_ipv6 = false
ipv6_subnet = "fd00::/80"
ipv6_gateway = "fd00::1"

# DNS
dns_servers = ["1.1.1.1", "8.8.8.8"]
dns_search = ["local"]

# Firewall
internal = false
enable_icc = true

# Labels
[network.config.labels]
"org.pdn.type" = "default"
"org.pdn.managed" = "true"
```

### CNI Configuration

PDN uses CNI for network plugins. CNI configs are stored in:

```
~/.config/pn-next/cni/
├── net.d/              # Network configurations
└── bin/                # CNI plugins
```

Example CNI config:
```json
{
    "cniVersion": "0.4.0",
    "name": "pdn-bridge",
    "type": "bridge",
    "bridge": "pdn0",
    "isGateway": true,
    "ipMasq": true,
    "ipam": {
        "type": "host-local",
        "subnet": "10.88.0.0/16",
        "routes": [{"dst": "0.0.0.0/0"}]
    }
}
```

## Storage Configuration

### `~/.config/pn-next/storage/<name>.toml`

```toml
# planned: created
# (the persisted volume file is a FLAT VolumeConfig: name, driver, labels, options —
#  there is no [storage.config] table)
[storage]
name = "local"
driver = "local"
created = "2024-01-15T10:30:00Z"

# planned: path, fs_type, mount_options, quota_enabled, quota_size
[storage.config]
# Local driver settings
path = "~/.local/share/pn-next/storage/local"
fs_type = "ext4"
mount_options = "defaults,noatime"

# Quotas
quota_enabled = true
quota_size = "50G"

# Labels
[storage.config.labels]
"org.pdn.type" = "local"
"org.pdn.managed" = "true"
```

## Environment Variable Overrides

Any configuration value can be overridden via environment variables:

```bash
# Global config
export PN_CONFIG_DIR=/custom/config
export PN_LOG_LEVEL=debug
export PN_GATEWAY_ADDR=unix:///custom/gateway.sock

# Container-specific (at runtime)
podnext run ubuntu --env PN_MEMORY_LIMIT=4G --env PN_CPU_LIMIT=4.0

# Security profile
export PN_SECURITY_PROFILE=restricted
```

## Configuration Validation

```bash
# Validate global config
podnext config validate

# Validate container config
podnext config validate --container ubuntu

# Show effective config (with overrides)
podnext config show ubuntu

# Generate default config
podnext config init
```

## Migrating Configuration

### From Docker Compose

```bash
# Convert docker-compose.yml to PDN configs
podnext config convert --from docker-compose --input docker-compose.yml --output ./pdn-configs/
```

### From Kubernetes

```bash
# Convert Kubernetes manifests
podnext config convert --from kubernetes --input deployment.yaml --output ./pdn-configs/
```

## See Also

- [Security](security.md) - Security profile details
- [CLI Reference](cli-reference.md) - Command-line options
- [Architecture](architecture.md) - System architecture
- [Migration Guide](migration-guide.md) - Migrating from PRoot-distro