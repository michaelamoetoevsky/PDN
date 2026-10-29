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
# Fail-safe configuration
[gateway.fail_safe]
circuit_breaker_threshold = 5
circuit_breaker_timeout = "60s"
retry_max_attempts = 3
retry_base_delay = "1s"
retry_max_delay = "30s"
bulkhead_max_concurrent = 10
bulkhead_queue_size = 100

# Guardrails
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
enabled = true
interval = "30s"
timeout = "10s"
healthy_threshold = 2
unhealthy_threshold = 3

# Metrics
[gateway.metrics]
enabled = true
port = 9090
path = "/metrics"
interval = "15s"

[grpc]
# gRPC server configuration
[grpc.server]
addr = "127.0.0.1:50051"
max_recv_msg_size = 4194304
max_send_msg_size = 4194304
keepalive_time = "20s"
keepalive_timeout = "10s"
keepalive_permit_without_stream = true

[grpc.client]
timeout = "30s"
max_recv_msg_size = 4194304
max_send_msg_size = 4194304

[api]
# REST API configuration
[api.server]
enabled = true
addr = "127.0.0.1:8080"
cors_allowed_origins = ["*"]
cors_allowed_methods = ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
cors_allowed_headers = ["*"]
cors_max_age = 3600

[tracing]
# OpenTelemetry tracing
[tracing.exporter]
type = "otlp"           # otlp, jaeger, zipkin, stdout
endpoint = "http://localhost:4317"
sampling_ratio = 0.1

[plugin]
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

## Container Configuration

### `~/.config/pn-next/containers/<name>.toml`

```toml
[container]
name = "ubuntu"
image = "docker.io/library/ubuntu:22.04"
created = "2024-01-15T10:30:00Z"
updated = "2024-01-15T10:30:00Z"

[container.config]
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
profile = "default"
custom_rules = []

[container.config.apparmor]
profile = "pn-next-default"
enforce = true

[container.config.selinux]
context = "system_u:system_r:container_t:s0:c123,c456"
enforce = true

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

[container.security]
profile = "default"
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

[container.network]
mode = "bridge"          # bridge, host, none, container:<name>
networks = ["default"]
dns = ["1.1.1.1", "8.8.8.8"]
dns_search = ["local"]
dns_options = ["ndots:2"]
extra_hosts = { "host.docker.internal" = "host-gateway" }
ports = []
ipc_mode = "shareable"
uts_mode = ""

[container.gpu]
enabled = false
devices = []
api = "vulkan"           # vulkan, opengl, opencl, cuda

[container.checkpoint]
enabled = false
interval = "30m"
max_checkpoints = 5
storage = "local"

[container.hooks]
prestart = []
poststart = []
prestop = []
poststop = []
```

## Security Profiles

### `~/.config/pn-next/security/<name>.toml`

```toml
[profile]
name = "default"
type = "default"         # default, restricted, unconfined, custom
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

[profile.apparmor]
profile = "pn-next-default"
enforce = true
# Custom AppArmor rules
custom_rules = [
    "network inet stream,",
    "network inet6 stream,",
    "capability setuid,",
    "capability setgid,"
]

[profile.selinux]
context = "system_u:system_r:container_t:s0:c123,c456"
enforce = true
# Custom SELinux rules
custom_rules = [
    "allow container_t self:capability { setuid setgid };",
    "allow container_t self:netlink_route_socket { nlmsg_read nlmsg_write };"
]

[profile.mask_paths]
paths = [
    "/proc/asound", "/proc/bus", "/proc/fs", "/proc/irq",
    "/proc/sys", "/proc/sysrq-trigger", "/proc/acpi",
    "/proc/kcore", "/proc/keys", "/proc/latency_stats",
    "/proc/sched_debug", "/proc/scsi", "/proc/timer_list",
    "/proc/timer_stats", "/sys/firmware", "/sys/kernel/security"
]

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
[network]
name = "default"
driver = "bridge"
created = "2024-01-15T10:30:00Z"

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
dns = ["1.1.1.1", "8.8.8.8"]
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
[storage]
name = "local"
driver = "local"
created = "2024-01-15T10:30:00Z"

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