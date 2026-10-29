# API Reference

PDN provides multiple API interfaces for programmatic access.

## Gateway API (gRPC)

### Service Definition

```protobuf
// proto/gateway.proto
syntax = "proto3";

package pdn.gateway.v1;

service Gateway {
    // Container management
    rpc CreateContainer(CreateContainerRequest) returns (CreateContainerResponse);
    rpc StartContainer(StartContainerRequest) returns (StartContainerResponse);
    rpc StopContainer(StopContainerRequest) returns (StopContainerResponse);
    rpc RemoveContainer(RemoveContainerRequest) returns (RemoveContainerResponse);
    rpc ListContainers(ListContainersRequest) returns (ListContainersResponse);
    rpc GetContainer(GetContainerRequest) returns (Container);
    
    // Image management
    rpc PullImage(PullImageRequest) returns (PullImageResponse);
    rpc PushImage(PushImageRequest) returns (PushImageResponse);
    rpc ListImages(ListImagesRequest) returns (ListImagesResponse);
    rpc RemoveImage(RemoveImageRequest) returns (RemoveImageResponse);
    rpc InspectImage(InspectImageRequest) returns (Image);
    
    // Network management
    rpc CreateNetwork(CreateNetworkRequest) returns (CreateNetworkResponse);
    rpc RemoveNetwork(RemoveNetworkRequest) returns (RemoveNetworkResponse);
    rpc ListNetworks(ListNetworksRequest) returns (ListNetworksResponse);
    rpc InspectNetwork(InspectNetworkRequest) returns (Network);
    
    // Volume management
    rpc CreateVolume(CreateVolumeRequest) returns (CreateVolumeResponse);
    rpc RemoveVolume(RemoveVolumeRequest) returns (RemoveVolumeResponse);
    rpc ListVolumes(ListVolumesRequest) returns (ListVolumesResponse);
    rpc InspectVolume(InspectVolumeRequest) returns (Volume);
    
    // Exec/Session
    rpc Exec(ExecRequest) returns (stream ExecResponse);
    rpc Attach(AttachRequest) returns (stream AttachResponse);
    
    // System
    rpc Info(InfoRequest) returns (InfoResponse);
    rpc Version(VersionRequest) returns (VersionResponse);
    rpc Health(HealthRequest) returns (HealthResponse);
    rpc Metrics(MetricsRequest) returns (stream MetricResponse);
}
```

### Connecting to Gateway

```rust
use tonic::transport::Channel;
use pdn_gateway::gateway_client::GatewayClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Unix socket connection
    let channel = Channel::builder("http://[::]:50051")
        .connect_with_connector(tower::service_fn(|_: Uri| {
            let path = "/run/pn-gateway.sock";
            tokio::net::UnixStream::connect(path)
        }))
        .await?;

    let mut client = GatewayClient::new(channel);
    
    // Use client...
    Ok(())
}
```

### Container Operations

```rust
// Create container
let response = client.create_container(CreateContainerRequest {
    name: "my-container".to_string(),
    image: "docker.io/library/ubuntu:22.04".to_string(),
    config: Some(ContainerConfig {
        hostname: "my-container".to_string(),
        working_dir: "/root".to_string(),
        env: vec!["TERM=xterm-256color".to_string()],
        mounts: vec![Mount {
            source: "/sdcard".to_string(),
            target: "/mnt/sdcard".to_string(),
            readonly: false,
        }],
        resources: Some(Resources {
            memory_limit: "2G".to_string(),
            cpu_limit: "2.0".to_string(),
            pids_limit: 100,
        }),
        security_profile: "default".to_string(),
    }),
}).await?;

let container_id = response.container_id;

// Start container
client.start_container(StartContainerRequest {
    container_id: container_id.clone(),
}).await?;

// Stop container
client.stop_container(StopContainerRequest {
    container_id: container_id.clone(),
    timeout: 10,
}).await?;

// Remove container
client.remove_container(RemoveContainerRequest {
    container_id,
    force: false,
    volumes: false,
}).await?;
```

### Image Operations

```rust
// Pull image
let response = client.pull_image(PullImageRequest {
    image: "docker.io/library/ubuntu:22.04".to_string(),
    auth: None,
    platform: None,
}).await?;

// List images
let response = client.list_images(ListImagesRequest {
    filter: "ubuntu".to_string(),
}).await?;

for image in response.images {
    println!("{} - {}", image.id, image.tags.join(", "));
}
```

### Exec Operations

```rust
use tokio_stream::StreamExt;

let mut stream = client.exec(ExecRequest {
    container_id: "container-id".to_string(),
    command: vec!["/bin/bash".to_string(), "-c".to_string(), "echo hello".to_string()],
    tty: false,
    env: vec!["TERM=xterm".to_string()],
    working_dir: "/root".to_string(),
    user: "root".to_string(),
}).await?;

while let Some(response) = stream.next().await {
    match response {
        Ok(resp) => {
            print!("{}", String::from_utf8_lossy(&resp.stdout));
            eprint!("{}", String::from_utf8_lossy(&resp.stderr));
            if resp.exit_code.is_some() {
                println!("Exit code: {}", resp.exit_code.unwrap());
            }
        }
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

## Gateway API (REST)

### Base URL

```
http://127.0.0.1:8080/api/v1
```

### Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/containers` | List containers |
| POST | `/containers` | Create container |
| GET | `/containers/{id}` | Get container |
| POST | `/containers/{id}/start` | Start container |
| POST | `/containers/{id}/stop` | Stop container |
| DELETE | `/containers/{id}` | Remove container |
| POST | `/containers/{id}/exec` | Execute command |
| GET | `/images` | List images |
| POST | `/images/pull` | Pull image |
| DELETE | `/images/{id}` | Remove image |
| GET | `/networks` | List networks |
| POST | `/networks` | Create network |
| DELETE | `/networks/{id}` | Remove network |
| GET | `/volumes` | List volumes |
| POST | `/volumes` | Create volume |
| DELETE | `/volumes/{id}` | Remove volume |
| GET | `/system/info` | System info |
| GET | `/system/version` | Version |
| GET | `/system/health` | Health check |
| GET | `/metrics` | Prometheus metrics |

### Examples

```bash
# List containers
curl http://127.0.0.1:8080/api/v1/containers

# Create container
curl -X POST http://127.0.0.1:8080/api/v1/containers \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-container",
    "image": "docker.io/library/ubuntu:22.04",
    "config": {
      "hostname": "my-container",
      "resources": {
        "memory_limit": "2G",
        "cpu_limit": "2.0"
      }
    }
  }'

# Start container
curl -X POST http://127.0.0.1:8080/api/v1/containers/<id>/start

# Execute command
curl -X POST http://127.0.0.1:8080/api/v1/containers/<id>/exec \
  -H "Content-Type: application/json" \
  -d '{
    "command": ["/bin/bash", "-c", "echo hello"],
    "tty": false
  }'

# Get metrics
curl http://127.0.0.1:8080/api/v1/metrics
```

## Runtime API (Rust)

### RuntimeManager

```rust
use pn_next::{RuntimeManager, ContainerConfig, Resources, SecurityProfile};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = RuntimeManager::new().await?;

    // Create sandbox
    let sandbox = runtime.create_sandbox(SandboxConfig {
        hostname: "sandbox".to_string(),
        network: "bridge".to_string(),
        ..Default::default()
    }).await?;

    // Create container
    let container = runtime.create_container(ContainerConfig {
        sandbox_id: sandbox.id,
        image: "docker.io/library/ubuntu:22.04".to_string(),
        config: Some(ContainerConfigInner {
            hostname: "container".to_string(),
            working_dir: "/root".to_string(),
            resources: Some(Resources {
                memory_limit: "2G".to_string(),
                cpu_limit: "2.0".to_string(),
                pids_limit: 100,
            }),
            security_profile: SecurityProfile::Default,
            ..Default::default()
        }),
    }).await?;

    // Start container
    runtime.start_container(&container.id).await?;

    // Exec
    let result = runtime.exec(container.id, ExecConfig {
        command: vec!["/bin/bash".to_string(), "-c".to_string(), "echo hello".to_string()],
        tty: false,
        ..Default::default()
    }).await?;

    println!("Exit code: {}", result.exit_code);
    println!("Stdout: {}", String::from_utf8_lossy(&result.stdout));
    println!("Stderr: {}", String::from_utf8_lossy(&result.stderr));

    // Stop and remove
    runtime.stop_container(&container.id, 10).await?;
    runtime.remove_container(&container.id).await?;

    Ok(())
}
```

### ContainerManager (proot-distro-core)

```rust
use proot_distro_core::{ContainerManager, ImageManager, NetworkManager, StorageManager};
use proot_distro_core::config::{ContainerConfig, InstallOptions, ImageSource};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let container_mgr = ContainerManager::new().await?;
    let image_mgr = ImageManager::new().await?;

    // Install container from image
    let container = container_mgr.install(InstallOptions {
        name: "ubuntu".to_string(),
        source: ImageSource::Docker("docker.io/library/ubuntu:22.04".to_string()),
        config: ContainerConfig::default(),
        distro_type: DistroType::Ubuntu,
    }).await?;

    // Start container
    container_mgr.start(&container.name).await?;

    // Run command
    let session = container_mgr.exec(&container.name, vec!["apt".to_string(), "update".to_string()]).await?;
    let output = session.wait_output().await?;
    println!("{}", output);

    // List containers
    let containers = container_mgr.list().await?;
    for c in containers {
        println!("{} - {}", c.name, c.status);
    }

    Ok(())
}
```

### ImageManager

```rust
use proot_distro_core::{ImageManager, ImageSource};

let image_mgr = ImageManager::new().await?;

// Search images
let results = image_mgr.search("ubuntu", Some(10)).await?;
for img in results {
    println!("{} - {}", img.name, img.description);
}

// Pull image
let image = image_mgr.pull(ImageSource::Docker("docker.io/library/alpine:latest".to_string())).await?;
println!("Pulled: {}", image.id);

// List local images
let images = image_mgr.list().await?;
for img in images {
    println!("{} - {}", img.id, img.tags.join(", "));
}
```

### NetworkManager

```rust
use proot_distro_core::{NetworkManager, NetworkConfig};

let network_mgr = NetworkManager::new().await?;

// Create network
let network = network_mgr.create(NetworkConfig {
    name: "mynet".to_string(),
    driver: "bridge".to_string(),
    subnet: Some("10.88.0.0/16".to_string()),
    gateway: Some("10.88.0.1".to_string()),
    ..Default::default()
}).await?;

// Connect container
network_mgr.connect(&network.name, "container-name").await?;

// Disconnect
network_mgr.disconnect(&network.name, "container-name").await?;

// Remove network
network_mgr.remove(&network.name).await?;
```

### StorageManager

```rust
use proot_distro_core::{StorageManager, VolumeConfig};

let storage_mgr = StorageManager::new().await?;

// Create volume
let volume = storage_mgr.create_volume(VolumeConfig {
    name: "my-data".to_string(),
    driver: "local".to_string(),
    size: Some("10G".to_string()),
    ..Default::default()
}).await?;

// Mount volume to container
// (done via container config mounts)

// Remove volume
storage_mgr.remove_volume(&volume.name).await?;
```

## Security API

### SecurityManager

```rust
use pn_next::{SecurityManager, SecurityProfileConfig, SeccompProfile, CapabilityConfig};

let security_mgr = SecurityManager::new().await?;

// Create security profile
let profile = SecurityProfileConfig {
    name: "my-profile".to_string(),
    profile_type: SecurityProfileType::Custom("my-profile".to_string()),
    enabled: true,
    seccomp: Some(SeccompProfile {
        default_action: SeccompAction::Errno,
        architectures: vec![SeccompArch::SCMP_ARCH_AARCH64],
        syscalls: vec![],
    }),
    capabilities: CapabilityConfig {
        drop: vec!["CAP_SYS_ADMIN".to_string()],
        add: vec![],
    },
    apparmor: Some(AppArmorProfile {
        profile: "pn-next-custom".to_string(),
        enforce: true,
    }),
    selinux: Some(SelinuxProfile {
        context: "system_u:system_r:container_t:s0:c123,c456".to_string(),
        enforce: true,
    }),
    no_new_privileges: true,
    read_only_rootfs: false,
    user_namespace: false,
};

// Apply to container
security_mgr.apply_profile(&container.id, &profile).await?;
```

## Plugin API

### Plugin Trait

```rust
use pdn_plugin_api::{Plugin, PluginMetadata, PluginType, Result};

#[async_trait]
pub trait Plugin: Send + Sync {
    fn metadata(&self) -> PluginMetadata;
    async fn initialize(&mut self, config: HashMap<String, String>) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
}

// Network plugin
#[async_trait]
pub trait NetworkPlugin: Plugin {
    async fn create_network(&self, config: NetworkConfig) -> Result<Network>;
    async fn delete_network(&self, name: &str) -> Result<()>;
    async fn connect_container(&self, network: &str, container: &str) -> Result<()>;
    async fn disconnect_container(&self, network: &str, container: &str) -> Result<()>;
}

// Storage plugin
#[async_trait]
pub trait StoragePlugin: Plugin {
    async fn create_volume(&self, config: VolumeConfig) -> Result<Volume>;
    async fn delete_volume(&self, name: &str) -> Result<()>;
    async fn mount_volume(&self, volume: &str, target: &str) -> Result<()>;
    async fn unmount_volume(&self, volume: &str) -> Result<()>;
}
```

## CRI gRPC API

### Service Definition

```protobuf
// Implements Kubernetes CRI v1alpha2/v1
service RuntimeService {
    rpc Version(VersionRequest) returns (VersionResponse);
    rpc RunPodSandbox(RunPodSandboxRequest) returns (RunPodSandboxResponse);
    rpc StopPodSandbox(StopPodSandboxRequest) returns (StopPodSandboxResponse);
    rpc RemovePodSandbox(RemovePodSandboxRequest) returns (RemovePodSandboxResponse);
    rpc PodSandboxStatus(PodSandboxStatusRequest) returns (PodSandboxStatusResponse);
    rpc ListPodSandbox(ListPodSandboxRequest) returns (ListPodSandboxResponse);
    rpc CreateContainer(CreateContainerRequest) returns (CreateContainerResponse);
    rpc StartContainer(StartContainerRequest) returns (StartContainerResponse);
    rpc StopContainer(StopContainerRequest) returns (StopContainerResponse);
    rpc RemoveContainer(RemoveContainerRequest) returns (RemoveContainerResponse);
    rpc ListContainers(ListContainersRequest) returns (ListContainersResponse);
    rpc ContainerStatus(ContainerStatusRequest) returns (ContainerStatusResponse);
    rpc ExecSync(ExecSyncRequest) returns (ExecSyncResponse);
    rpc Exec(ExecRequest) returns (ExecResponse);
    rpc Attach(AttachRequest) returns (AttachResponse);
    rpc PortForward(PortForwardRequest) returns (PortForwardResponse);
    rpc ContainerStats(ContainerStatsRequest) returns (ContainerStatsResponse);
    rpc ListContainerStats(ListContainerStatsRequest) returns (ListContainerStatsResponse);
    rpc UpdateContainerResources(UpdateContainerResourcesRequest) returns (UpdateContainerResourcesResponse);
    rpc Status(StatusRequest) returns (StatusResponse);
}
```

### Usage with Containerd

```toml
# /etc/containerd/config.toml
[plugins."io.containerd.grpc.v1.cri".containerd.runtimes.pdn]
  runtime_type = "io.containerd.pdn.v1"
  [plugins."io.containerd.grpc.v1.cri".containerd.runtimes.pdn.options]
    BinaryName = "podnext"
    RootDir = "/run/pdn"
```

## Metrics API

### Prometheus Metrics

```bash
# Scrape metrics
curl http://127.0.0.1:9090/metrics
```

### Key Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `pdn_containers_total` | Gauge | Total containers |
| `pdn_containers_running` | Gauge | Running containers |
| `pdn_images_total` | Gauge | Total images |
| `pdn_networks_total` | Gauge | Total networks |
| `pdn_volumes_total` | Gauge | Total volumes |
| `pdn_container_cpu_seconds_total` | Counter | CPU usage per container |
| `pdn_container_memory_bytes` | Gauge | Memory usage per container |
| `pdn_container_network_rx_bytes_total` | Counter | Network RX per container |
| `pdn_container_network_tx_bytes_total` | Counter | Network TX per container |
| `pdn_gateway_requests_total` | Counter | Gateway requests |
| `pdn_gateway_request_duration_seconds` | Histogram | Request latency |
| `pdn_guardrails_violations_total` | Counter | Guardrails violations |
| `pdn_fail_safe_circuit_breaker_state` | Gauge | Circuit breaker state |

## Authentication

### Gateway Authentication

```bash
# Bearer token
export PN_GATEWAY_TOKEN="your-token"
curl -H "Authorization: Bearer $PN_GATEWAY_TOKEN" http://127.0.0.1:8080/api/v1/containers
```

### mTLS (Planned)

```rust
let channel = Channel::builder("https://[::]:50051")
    .tls_config(ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(ca_cert))
        .identity(Identity::from_pem(client_cert, client_key))
    )?
    .connect()
    .await?;
```

## Error Handling

### gRPC Errors

```rust
use tonic::{Code, Status};

match client.create_container(request).await {
    Ok(response) => { /* success */ }
    Err(status) => {
        match status.code() {
            Code::AlreadyExists => { /* container exists */ }
            Code::NotFound => { /* image not found */ }
            Code::ResourceExhausted => { /* quota exceeded */ }
            Code::PermissionDenied => { /* security policy */ }
            Code::Internal => { /* internal error */ }
            _ => { /* other */ }
        }
    }
}
```

### REST Errors

```json
{
    "error": {
        "code": "CONTAINER_NOT_FOUND",
        "message": "Container 'my-container' not found",
        "details": {}
    }
}
```

### Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `CONTAINER_NOT_FOUND` | 404 | Container doesn't exist |
| `IMAGE_NOT_FOUND` | 404 | Image doesn't exist |
| `CONTAINER_ALREADY_EXISTS` | 409 | Name conflict |
| `RESOURCE_EXHAUSTED` | 429 | Quota exceeded |
| `SECURITY_VIOLATION` | 403 | Policy violation |
| `INVALID_CONFIG` | 400 | Invalid configuration |
| `INTERNAL_ERROR` | 500 | Internal error |
| `GATEWAY_UNAVAILABLE` | 503 | Gateway down |

## See Also

- [CLI Reference](cli-reference.md) - Command-line interface
- [Configuration](configuration.md) - API configuration
- [Security](security.md) - API security
- [Development](development.md) - Building and testing