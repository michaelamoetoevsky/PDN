//! Gateway configuration

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub gateway_id: Uuid,
    pub name: String,
    pub version: String,
    pub proot_config: ProotConfig,
    pub distro_config: DistroConfig,
    pub compat_config: CompatConfig,
    pub fail_safe: FailSafeConfig,
    pub guardrails: GuardrailConfig,
    pub health: HealthConfig,
    pub metrics: MetricsConfig,
    pub network: NetworkConfig,
    pub storage: StorageConfig,
    pub logging: LoggingConfig,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            gateway_id: uuid::Uuid::new_v4(),
            name: "pn-gateway".to_string(),
            version: "0.1.0".to_string(),
            proot_config: ProotConfig::default(),
            distro_config: DistroConfig::default(),
            compat_config: CompatConfig::default(),
            fail_safe: FailSafeConfig::default(),
            guardrails: GuardrailConfig::default(),
            health: HealthConfig::default(),
            metrics: MetricsConfig::default(),
            network: NetworkConfig::default(),
            storage: StorageConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProotConfig {
    pub rootfs: Option<std::path::PathBuf>,
    pub bind_mounts: Vec<BindMount>,
    pub env_vars: std::collections::HashMap<String, String>,
    pub seccomp_enabled: bool,
    pub syscall_trace: bool,
    pub max_tracees: usize,
    pub timeout: Duration,
}

impl Default for ProotConfig {
    fn default() -> Self {
        Self {
            rootfs: None,
            bind_mounts: vec![
                BindMount { source: "/proc".into(), target: "/proc".into(), readonly: true },
                BindMount { source: "/sys".into(), target: "/sys".into(), readonly: true },
                BindMount { source: "/dev".into(), target: "/dev".into(), readonly: false },
                BindMount { source: "/run".into(), target: "/run".into(), readonly: false },
                BindMount { source: "/tmp".into(), target: "/tmp".into(), readonly: false },
            ],
            env_vars: std::collections::HashMap::new(),
            seccomp_enabled: true,
            syscall_trace: false,
            max_tracees: 1000,
            timeout: Duration::from_secs(300),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindMount {
    pub source: std::path::PathBuf,
    pub target: std::path::PathBuf,
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistroConfig {
    pub default_distro: String,
    pub distro_sources: Vec<DistroSource>,
    pub default_arch: String,
    pub auto_update: bool,
    pub cache_dir: std::path::PathBuf,
}

impl Default for DistroConfig {
    fn default() -> Self {
        Self {
            default_distro: "ubuntu:22.04".to_string(),
            distro_sources: vec![
                DistroSource {
                    name: "ubuntu".to_string(),
                    versions: vec!["20.04".to_string(), "22.04".to_string(), "24.04".to_string()],
                    base_url: "https://cloud-images.ubuntu.com/minimal/releases".to_string(),
                },
                DistroSource {
                    name: "debian".to_string(),
                    versions: vec!["11".to_string(), "12".to_string()],
                    base_url: "https://cloud.debian.org/images/cloud".to_string(),
                },
                DistroSource {
                    name: "alpine".to_string(),
                    versions: vec!["3.18".to_string(), "3.19".to_string()],
                    base_url: "https://dl-cdn.alpinelinux.org/alpine".to_string(),
                },
            ],
            default_arch: "aarch64".to_string(),
            auto_update: true,
            cache_dir: std::path::PathBuf::from("/var/cache/pn-gateway"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistroSource {
    pub name: String,
    pub versions: Vec<String>,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatConfig {
    pub platform_shim_enabled: bool,
    pub path_translation_enabled: bool,
    pub seccomp_bypass_enabled: bool,
    pub stub_fs_enabled: bool,
    pub mount_helper_enabled: bool,
    pub cgroup_v2_enabled: bool,
    pub custom_stub_paths: Vec<String>,
    pub custom_mount_points: Vec<String>,
}

impl Default for CompatConfig {
    fn default() -> Self {
        Self {
            platform_shim_enabled: true,
            path_translation_enabled: true,
            seccomp_bypass_enabled: true,
            stub_fs_enabled: true,
            mount_helper_enabled: true,
            cgroup_v2_enabled: true,
            custom_stub_paths: vec![
                "/run/dbus".to_string(),
                "/tmp".to_string(),
                "/sys/fs/cgroup".to_string(),
                "/sys/class".to_string(),
                "/dev/shm".to_string(),
            ],
            custom_mount_points: vec![
                "/proc".to_string(),
                "/sys".to_string(),
                "/dev".to_string(),
                "/run".to_string(),
                "/tmp".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailSafeConfig {
    pub enabled: bool,
    pub max_retries: u32,
    pub retry_delay: Duration,
    pub fallback_actions: Vec<FailSafeAction>,
    pub circuit_breaker: CircuitBreakerConfig,
    pub auto_recovery: bool,
}

impl Default for FailSafeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_retries: 3,
            retry_delay: Duration::from_secs(5),
            fallback_actions: vec![
                FailSafeAction::Retry,
                FailSafeAction::FallbackToHost,
                FailSafeAction::RestartContainer,
            ],
            circuit_breaker: CircuitBreakerConfig::default(),
            auto_recovery: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    pub enabled: bool,
    pub failure_threshold: u32,
    pub timeout: Duration,
    pub half_open_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            failure_threshold: 5,
            timeout: Duration::from_secs(60),
            half_open_requests: 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

impl Default for CircuitBreakerState {
    fn default() -> Self {
        Self::Closed
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailSafeAction {
    Retry,
    FallbackToHost,
    RestartContainer,
    RestartGateway,
    GracefulShutdown,
    AlertAdmin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailConfig {
    pub enabled: bool,
    pub rules: Vec<GuardrailRule>,
    pub strict_mode: bool,
    pub audit_log: bool,
}

impl Default for GuardrailConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rules: vec![
                GuardrailRule {
                    name: "max_containers".to_string(),
                    description: "Maximum number of concurrent containers".to_string(),
                    condition: "container_count > 100".to_string(),
                    action: GuardrailAction::Deny,
                    severity: Severity::Critical,
                },
                GuardrailRule {
                    name: "memory_limit".to_string(),
                    description: "Memory usage limit".to_string(),
                    condition: "memory_usage > 80%".to_string(),
                    action: GuardrailAction::Warn,
                    severity: Severity::Warning,
                },
                GuardrailRule {
                    name: "cpu_limit".to_string(),
                    description: "CPU usage limit".to_string(),
                    condition: "cpu_usage > 90%".to_string(),
                    action: GuardrailAction::Throttle,
                    severity: Severity::Warning,
                },
                GuardrailRule {
                    name: "disk_space".to_string(),
                    description: "Disk space limit".to_string(),
                    condition: "disk_usage > 90%".to_string(),
                    action: GuardrailAction::Deny,
                    severity: Severity::Critical,
                },
                GuardrailRule {
                    name: "privileged_access".to_string(),
                    description: "Prevent privileged container access".to_string(),
                    condition: "privileged == true".to_string(),
                    action: GuardrailAction::Deny,
                    severity: Severity::Critical,
                },
            ],
            strict_mode: true,
            audit_log: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailRule {
    pub name: String,
    pub description: String,
    pub condition: String,
    pub action: GuardrailAction,
    pub severity: Severity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GuardrailAction {
    Allow,
    Warn,
    Deny,
    Throttle,
    Quarantine,
    Alert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    pub enabled: bool,
    pub check_interval: Duration,
    pub checks: Vec<HealthCheck>,
    pub unhealthy_threshold: u32,
    pub recovery_actions: Vec<RecoveryAction>,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval: Duration::from_secs(30),
            checks: vec![
                HealthCheck {
                    name: "proot_process".to_string(),
                    check_type: HealthCheckType::Process,
                    target: "proot".to_string(),
                    interval: Duration::from_secs(10),
                    timeout: Duration::from_secs(5),
                },
                HealthCheck {
                    name: "disk_space".to_string(),
                    check_type: HealthCheckType::DiskSpace,
                    target: "/".to_string(),
                    interval: Duration::from_secs(60),
                    timeout: Duration::from_secs(5),
                },
                HealthCheck {
                    name: "memory_usage".to_string(),
                    check_type: HealthCheckType::MemoryUsage,
                    target: "system".to_string(),
                    interval: Duration::from_secs(30),
                    timeout: Duration::from_secs(5),
                },
                HealthCheck {
                    name: "cpu_usage".to_string(),
                    check_type: HealthCheckType::CpuUsage,
                    target: "system".to_string(),
                    interval: Duration::from_secs(30),
                    timeout: Duration::from_secs(5),
                },
                HealthCheck {
                    name: "disk_space".to_string(),
                    check_type: HealthCheckType::DiskSpace,
                    target: "/".to_string(),
                    interval: Duration::from_secs(60),
                    timeout: Duration::from_secs(5),
                },
                HealthCheck {
                    name: "network_connectivity".to_string(),
                    check_type: HealthCheckType::NetworkConnectivity,
                    target: "8.8.8.8:53".to_string(),
                    interval: Duration::from_secs(60),
                    timeout: Duration::from_secs(10),
                },
            ],
            unhealthy_threshold: 3,
            recovery_actions: vec![
                RecoveryAction::RestartUnhealthyService,
                RecoveryAction::RestartGateway,
                RecoveryAction::AlertAdmin,
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub check_type: HealthCheckType,
    pub target: String,
    pub interval: Duration,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthCheckType {
    Process,
    DiskSpace,
    MemoryUsage,
    CpuUsage,
    NetworkConnectivity,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryAction {
    RestartUnhealthyService,
    RestartGateway,
    AlertAdmin,
    DrainConnections,
    Failover,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub interval: Duration,
    pub retention: Duration,
    pub exporters: Vec<MetricsExporter>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(60),
            retention: Duration::from_secs(86400 * 7), // 7 days
            exporters: vec![
                MetricsExporter::Prometheus,
                MetricsExporter::JsonFile,
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricsExporter {
    Prometheus,
    JsonFile,
    InfluxDB,
    StatsD,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub enabled: bool,
    pub bridge_name: String,
    pub subnet: String,
    pub dns_servers: Vec<String>,
    pub port_mappings: Vec<PortMapping>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bridge_name: "pn0".to_string(),
            subnet: "10.0.0.0/24".to_string(),
            dns_servers: vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()],
            port_mappings: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub root_dir: std::path::PathBuf,
    pub max_size: u64,
    pub cleanup_interval: Duration,
    pub backup_enabled: bool,
    pub backup_interval: Duration,
    pub backup_retention: u32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            root_dir: std::path::PathBuf::from("/var/lib/pn-gateway"),
            max_size: 50 * 1024 * 1024 * 1024, // 50 GB
            cleanup_interval: Duration::from_secs(3600),
            backup_enabled: true,
            backup_interval: Duration::from_secs(86400), // daily
            backup_retention: 7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
    pub output: Vec<String>,
    pub file_rotation: FileRotationConfig,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "json".to_string(),
            output: vec!["stdout".to_string(), "file".to_string()],
            file_rotation: FileRotationConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRotationConfig {
    pub enabled: bool,
    pub max_size: u64,
    pub max_files: u32,
    pub compress: bool,
}

impl Default for FileRotationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_size: 100 * 1024 * 1024, // 100 MB
            max_files: 10,
            compress: true,
        }
    }
}

/// Gateway state
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GatewayState {
    Initializing,
    Ready,
    Running,
    Degraded,
    ShuttingDown,
    Stopped,
    Error,
}

/// Builder for GatewayConfig
pub struct GatewayConfigBuilder {
    config: GatewayConfig,
}

impl GatewayConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: GatewayConfig::default(),
        }
    }
    
    pub fn name(mut self, name: String) -> Self {
        self.config.name = name;
        self
    }
    
    pub fn version(mut self, version: String) -> Self {
        self.config.version = version;
        self
    }
    
    pub fn proot_config(mut self, config: ProotConfig) -> Self {
        self.config.proot_config = config;
        self
    }
    
    pub fn distro_config(mut self, config: DistroConfig) -> Self {
        self.config.distro_config = config;
        self
    }
    
    pub fn compat_config(mut self, config: CompatConfig) -> Self {
        self.config.compat_config = config;
        self
    }
    
    pub fn fail_safe(mut self, config: FailSafeConfig) -> Self {
        self.config.fail_safe = config;
        self
    }
    
    pub fn guardrails(mut self, config: GuardrailConfig) -> Self {
        self.config.guardrails = config;
        self
    }
    
    pub fn health(mut self, config: HealthConfig) -> Self {
        self.config.health = config;
        self
    }
    
    pub fn metrics(mut self, config: MetricsConfig) -> Self {
        self.config.metrics = config;
        self
    }
    
    pub fn network(mut self, config: NetworkConfig) -> Self {
        self.config.network = config;
        self
    }
    
    pub fn storage(mut self, config: StorageConfig) -> Self {
        self.config.storage = config;
        self
    }
    
    pub fn logging(mut self, config: LoggingConfig) -> Self {
        self.config.logging = config;
        self
    }
    
    pub fn build(self) -> GatewayConfig {
        self.config
    }
}

impl Default for GatewayConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}