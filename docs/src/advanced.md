# Advanced Topics

## Plugin System

PDN supports dynamic plugins via `libloading` for extending functionality.

### Plugin Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     pn-next Core                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │              PluginManager                          │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐            │    │
│  │  │ Network  │ │ Storage  │ │ Runtime  │  Plugins   │    │
│  │  │ Plugin   │ │ Plugin   │ │ Plugin   │            │    │
│  │  └──────────┘ └──────────┘ └──────────┘            │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

### Plugin Types

| Type | Interface | Description |
|------|-----------|-------------|
| Network | `NetworkPlugin` | CNI plugins, service mesh |
| Storage | `StoragePlugin` | CSI drivers, backup |
| Runtime | `RuntimePlugin` | Alternative runtimes (runc, crun, kata) |
| Auth | `AuthPlugin` | Authentication providers |
| Metrics | `MetricsPlugin` | Custom metrics exporters |
| Hook | `HookPlugin` | Lifecycle hooks |

### Creating a Plugin

```rust
// Cargo.toml
[package]
name = "pdn-plugin-myplugin"
version = "0.1.0"
edition = "2021"
crate-type = ["cdylib"]

[dependencies]
pdn-plugin-api = { path = "../../crates/pn-next/pn-plugin-api" }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
async-trait = "0.1"
```

```rust
// src/lib.rs
use pdn_plugin_api::{Plugin, PluginMetadata, PluginType, Result};
use async_trait::async_trait;
use std::collections::HashMap;

pub struct MyNetworkPlugin {
    config: HashMap<String, String>,
}

#[async_trait]
impl Plugin for MyNetworkPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "my-network-plugin".to_string(),
            version: "0.1.0".to_string(),
            author: "My Name".to_string(),
            description: "Custom network plugin".to_string(),
            plugin_type: PluginType::Network,
            api_version: "1.0".to_string(),
        }
    }

    async fn initialize(&mut self, config: HashMap<String, String>) -> Result<()> {
        self.config = config;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

// Network-specific implementation
#[async_trait]
impl NetworkPlugin for MyNetworkPlugin {
    async fn create_network(&self, config: NetworkConfig) -> Result<Network> {
        // Implementation
        Ok(Network::default())
    }

    async fn delete_network(&self, name: &str) -> Result<()> {
        Ok(())
    }

    async fn connect_container(&self, network: &str, container: &str) -> Result<()> {
        Ok(())
    }

    async fn disconnect_container(&self, network: &str, container: &str) -> Result<()> {
        Ok(())
    }
}

// Export plugin
#[no_mangle]
pub extern "C" fn create_plugin() -> Box<dyn Plugin> {
    Box::new(MyNetworkPlugin::default())
}
```

### Installing Plugins

```bash
# Build plugin
cargo build --release --package pdn-plugin-myplugin

# Install to plugin directory
cp target/release/libpdn_plugin_myplugin.so ~/.config/pn-next/plugins/

# Or system-wide
sudo cp target/release/libpdn_plugin_myplugin.so /usr/lib/pn-next/plugins/

# Enable in config
# ~/.config/pn-next/config.toml
[plugin.manager]
enabled = true
plugin_dirs = ["~/.config/pn-next/plugins", "/usr/lib/pn-next/plugins"]
```

### Using Plugins

```bash
# List plugins
podnext plugin list

# Enable/disable
podnext plugin enable my-network-plugin
podnext plugin disable my-network-plugin

# Plugin-specific commands
podnext network create --plugin my-network-plugin mynet
```

## Checkpoint/Restore (CRIU)

### Overview

CRIU (Checkpoint/Restore In Userspace) allows freezing a running container and restoring it later.

### Requirements

- Linux kernel 5.10+ with `CONFIG_CHECKPOINT_RESTORE=y`
- CRIU 3.15+ installed
- Container running with `--security-profile unconfined` or appropriate seccomp

### Basic Usage

```bash
# Start container
podnext run ubuntu --detach --name myapp -- nginx -g "daemon off;"

# Checkpoint container
podnext checkpoint myapp --name checkpoint-1

# List checkpoints
podnext checkpoint list myapp

# Restore from checkpoint
podnext restore checkpoint-1 --name myapp-restored

# Restore to different container
podnext restore checkpoint-1 --name myapp-copy --config new-config.toml
```

### Configuration

```toml
[container.checkpoint]
enabled = true
interval = "30m"              # Auto-checkpoint interval
max_checkpoints = 5           # Max checkpoints to keep
storage = "local"             # Storage backend
compress = true               # Compress checkpoint
encrypt = false               # Encrypt checkpoint
```

### Programmatic API

```rust
use pn_next::{RuntimeManager, CheckpointOptions, RestoreOptions};

let runtime = RuntimeManager::new().await?;

// Checkpoint
let checkpoint = runtime.checkpoint(container_id, CheckpointOptions {
    name: "checkpoint-1".to_string(),
    compress: true,
    encrypt: false,
    storage: "local".to_string(),
}).await?;

// Restore
let container = runtime.restore(RestoreOptions {
    checkpoint_id: checkpoint.id,
    name: "restored-container".to_string(),
    config: Some(new_config),
}).await?;
```

### Limitations

- Requires `CAP_SYS_ADMIN` and `CAP_CHECKPOINT_RESTORE`
- Not all syscalls supported (seccomp must allow checkpoint/restore)
- GPU state not preserved
- Network connections may not restore cleanly
- File descriptors to host paths not preserved

## GPU Acceleration

### Supported APIs

| API | Description | Requirements |
|-----|-------------|--------------|
| Vulkan | Compute & graphics | Vulkan loader, ICD |
| OpenGL | Graphics rendering | Mesa, EGL |
| OpenCL | Compute | ICD loader |
| CUDA | NVIDIA compute | NVIDIA driver, toolkit |

### Device Detection

```bash
# List GPU devices
podnext info --gpu

# Output:
# GPU Devices:
#   /dev/dri/renderD128 - Adreno 730 (Vulkan, OpenGL, OpenCL)
#   /dev/dri/card0 - Adreno 730 (Display)
```

### Configuration

```toml
[container.gpu]
enabled = true
devices = [
    "/dev/dri/renderD128",
    "/dev/dri/card0"
]
api = "vulkan"           # vulkan, opengl, opencl, cuda

# Vendor-specific
[container.gpu.vulkan]
icd_paths = [
    "/usr/share/vulkan/icd.d/adreno_icd.aarch64.json",
    "/usr/share/vulkan/icd.d/adreno_icd.json"
]
layers = []

[container.gpu.opengl]
egl_platforms = ["wayland", "x11", "surfaceless"]
```

### Usage

```bash
# Run with GPU
podnext run ubuntu --gpu -- vulkaninfo

# Run OpenGL app
podnext run ubuntu --gpu -- glxinfo | grep "OpenGL renderer"

# Run Vulkan app
podnext run ubuntu --gpu -- vkcube

# Run with specific GPU
podnext run ubuntu --gpu --device /dev/dri/renderD128 -- vulkaninfo
```

### Android GPU Support

On Termux/Android, GPU access requires:

```bash
# Required packages
pkg install vulkan-loader vulkan-tools mesa-gl mesa-vulkan

# Environment
export VK_ICD_FILENAMES=/system/lib64/vulkan/adreno_icd.aarch64.json
export MESA_LOADER_DRIVER_OVERRIDE=kgsl
```

### GPU in Containers

```dockerfile
# Dockerfile for GPU-enabled container
FROM ubuntu:22.04

RUN apt-get update && apt-get install -y \
    vulkan-tools \
    mesa-vulkan-drivers \
    libgl1-mesa-glx \
    && rm -rf /var/lib/apt/lists/*

# Set Vulkan ICD
ENV VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.x86_64.json:/usr/share/vulkan/icd.d/radeon_icd.json

CMD ["vulkaninfo"]
```

## Multi-arch Image Support

### Manifest Lists

PDN supports Docker manifest lists (multi-arch images):

```bash
# Pull multi-arch image (auto-selects correct arch)
podnext install docker.io/library/ubuntu:22.04

# Force specific architecture
podnext install docker.io/library/ubuntu:22.04 --platform linux/arm64
podnext install docker.io/library/ubuntu:22.04 --platform linux/amd64
```

### Building Multi-arch Images

```bash
# Using buildx
docker buildx create --use --name pdn-builder
docker buildx build --platform linux/arm64,linux/amd64 \
  -t myorg/myimage:tag --push .

# PDN can build (planned)
podnext build --platform linux/arm64,linux/amd64 -t myorg/myimage:tag .
```

### Platform-Specific Configuration

```toml
[container]
image = "docker.io/library/ubuntu:22.04"

# Platform-specific overrides
[container.platform."linux/arm64"]
resources.memory_limit = "1G"
config.env.ARCH = "arm64"

[container.platform."linux/amd64"]
resources.memory_limit = "2G"
config.env.ARCH = "amd64"
```

## CNI Plugin Support

### Supported CNI Plugins

| Plugin | Type | Description |
|--------|------|-------------|
| bridge | Main | Linux bridge |
| host-device | Main | Host device passthrough |
| ipvlan | Main | IPvlan |
| macvlan | Main | Macvlan |
| ptp | Main | Point-to-point |
| vlan | Main | VLAN |
| host-local | IPAM | Local IP allocation |
| dhcp | IPAM | DHCP |
| static | IPAM | Static IP |
| whereabouts | IPAM | Distributed IPAM |
| flannel | Overlay | VXLAN overlay |
| calico | Overlay | BGP/Overlay |
| weave | Overlay | Mesh overlay |
| multus | Meta | Multiple interfaces |
| portmap | Port mapping | Port forwarding |
| bandwidth | Traffic shaping | Bandwidth limits |
| firewall | Security | Network policies |
| sbr | Source routing | Source-based routing |

### CNI Configuration

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
        "routes": [{"dst": "0.0.0.0/0"}],
        "ranges": [[{"subnet": "10.88.0.0/16", "rangeStart": "10.88.1.1", "rangeEnd": "10.88.254.254"}]]
    },
    "dns": {
        "nameservers": ["1.1.1.1", "8.8.8.8"],
        "domain": "local",
        "search": ["local"]
    }
}
```

### Using CNI Plugins

```bash
# Create network with CNI config
podnext network create mynet --cni-config ./my-cni-config.json

# Use multus for multiple interfaces
podnext network create multus-net --cni-config ./multus.conf

# Use whereabouts for IPAM
podnext network create whereabouts-net --cni-config ./whereabouts.conf
```

### Custom CNI Plugins

```bash
# Install CNI plugin
cp my-cni-plugin ~/.config/pn-next/cni/bin/
chmod +x ~/.config/pn-next/cni/bin/my-cni-plugin

# Use in network config
{
    "type": "my-cni-plugin",
    "custom_param": "value"
}
```

## Service Mesh Integration

### Planned Features

- mTLS between containers
- Traffic splitting
- Circuit breaking
- Retry policies
- Rate limiting
- Observability (metrics, traces, logs)

### Envoy Integration (Planned)

```toml
[container.network.service_mesh]
enabled = true
sidecar = "envoy"
config = """
static_resources:
  listeners:
  - name: inbound
    address:
      socket_address:
        address: 0.0.0.0
        port_value: 15000
    filter_chains:
    - filters:
      - name: envoy.filters.network.http_connection_manager
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
          stat_prefix: inbound
          route_config:
            name: local_route
            virtual_hosts:
            - name: local_service
              domains: ["*"]
              routes:
              - match:
                  prefix: "/"
                route:
                  cluster: local_service
          http_filters:
          - name: envoy.filters.http.router
  clusters:
  - name: local_service
    connect_timeout: 0.25s
    type: STATIC
    load_assignment:
      cluster_name: local_service
      endpoints:
      - lb_endpoints:
        - endpoint:
            address:
              socket_address:
                address: 127.0.0.1
                port_value: 8080
"""
```

## Workflow Engine

### DAG-based Workflows

```yaml
# workflow.yaml
name: "build-and-deploy"
version: "1.0"

tasks:
  - name: "build"
    image: "docker.io/library/rust:1.75"
    command: ["cargo", "build", "--release"]
    volumes:
      - "source:/workspace"
    resources:
      cpu: "2.0"
      memory: "4G"

  - name: "test"
    image: "docker.io/library/rust:1.75"
    command: ["cargo", "test", "--release"]
    depends_on: ["build"]
    volumes:
      - "source:/workspace"
    resources:
      cpu: "2.0"
      memory: "4G"

  - name: "docker-build"
    image: "docker.io/library/docker:24-dind"
    command: ["docker", "build", "-t", "myapp:latest", "."]
    depends_on: ["test"]
    volumes:
      - "source:/workspace"
      - "/var/run/docker.sock:/var/run/docker.sock"
    security_profile: "unconfined"
    resources:
      cpu: "2.0"
      memory: "4G"

  - name: "deploy"
    image: "docker.io/library/kubectl:latest"
    command: ["kubectl", "set", "image", "deployment/myapp", "myapp=myapp:latest"]
    depends_on: ["docker-build"]
    env:
      KUBECONFIG: "/kubeconfig"
    volumes:
      - "kubeconfig:/kubeconfig"
```

### Running Workflows

```bash
# Run workflow
podnext workflow run workflow.yaml

# List workflows
podnext workflow list

# Get workflow status
podnext workflow status <workflow-id>

# Cancel workflow
podnext workflow cancel <workflow-id>

# View workflow logs
podnext workflow logs <workflow-id>
```

## OCI Runtime Spec Compliance

### CRI gRPC API

PDN implements the Kubernetes Container Runtime Interface (CRI) gRPC API:

```bash
# Start CRI server
podnext cri serve --socket /run/pdn-cri.sock

# Configure kubelet
# /etc/kubernetes/kubelet.conf
containerRuntimeEndpoint: unix:///run/pdn-cri.sock
```

### Supported CRI Methods

| Method | Status | Description |
|--------|--------|-------------|
| Version | ✅ | Runtime version |
| RunPodSandbox | ✅ | Create pod sandbox |
| StopPodSandbox | ✅ | Stop pod sandbox |
| RemovePodSandbox | ✅ | Remove pod sandbox |
| PodSandboxStatus | ✅ | Get pod status |
| ListPodSandbox | ✅ | List pod sandboxes |
| CreateContainer | ✅ | Create container |
| StartContainer | ✅ | Start container |
| StopContainer | ✅ | Stop container |
| RemoveContainer | ✅ | Remove container |
| ListContainers | ✅ | List containers |
| ContainerStatus | ✅ | Get container status |
| UpdateContainerResources | ✅ | Update resources |
| ExecSync | ✅ | Execute command (sync) |
| Exec | ✅ | Execute command (async) |
| Attach | ✅ | Attach to container |
| PortForward | ✅ | Port forward |
| ContainerStats | ✅ | Container stats |
| ListContainerStats | ✅ | List container stats |
| UpdateRuntimeConfig | ✅ | Update runtime config |
| Status | ✅ | Runtime status |

### Containerd Integration

```toml
# /etc/containerd/config.toml
[plugins."io.containerd.grpc.v1.cri".containerd.runtimes.pdn]
  runtime_type = "io.containerd.pdn.v1"
  privileged_without_host_devices = false
  base_runtime_spec = ""
  [plugins."io.containerd.grpc.v1.cri".containerd.runtimes.pdn.options]
    BinaryName = "podnext"
    RootDir = "/run/pdn"
    SystemdCgroup = false
```

## See Also

- [CLI Reference](cli-reference.md) - Plugin and workflow commands
- [Configuration](configuration.md) - Plugin and advanced config
- [Security](security.md) - Security for advanced features
- [API Reference](api-reference.md) - Programmatic APIs