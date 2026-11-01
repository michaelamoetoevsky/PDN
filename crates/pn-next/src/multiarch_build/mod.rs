//! Multi-arch Build Service Module
//! Cross-compilation and multi-architecture build service for PDN

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::fs;
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Multi-arch build configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildServiceConfig {
    /// Enable build service
    pub enabled: bool,
    /// Build workspace directory
    pub workspace_dir: PathBuf,
    /// Artifact storage directory
    pub artifact_dir: PathBuf,
    /// Maximum concurrent builds
    pub max_concurrent_builds: usize,
    /// Build timeout (seconds)
    pub build_timeout_secs: u64,
    /// Supported target architectures
    pub target_architectures: Vec<TargetArch>,
    /// Default target architecture
    pub default_target: TargetArch,
    /// Builder images
    pub builder_images: HashMap<TargetArch, String>,
    /// Registry for pushing images
    pub registry: RegistryConfig,
    /// Cache configuration
    pub cache: CacheConfig,
    /// Build hooks
    pub hooks: BuildHooks,
    /// Resource limits for builds
    pub resource_limits: BuildResourceLimits,
}

impl Default for BuildServiceConfig {
    fn default() -> Self {
        let mut builder_images = HashMap::new();
        builder_images.insert(TargetArch::Amd64, "pdn-builder-amd64:latest".to_string());
        builder_images.insert(TargetArch::Arm64, "pdn-builder-arm64:latest".to_string());
        builder_images.insert(TargetArch::ArmV7, "pdn-builder-armv7:latest".to_string());
        builder_images.insert(
            TargetArch::RiscV64,
            "pdn-builder-riscv64:latest".to_string(),
        );
        builder_images.insert(
            TargetArch::Ppc64le,
            "pdn-builder-ppc64le:latest".to_string(),
        );
        builder_images.insert(TargetArch::S390x, "pdn-builder-s390x:latest".to_string());

        Self {
            enabled: true,
            workspace_dir: PathBuf::from("/var/lib/pdn/builds"),
            artifact_dir: PathBuf::from("/var/lib/pdn/artifacts"),
            max_concurrent_builds: 4,
            build_timeout_secs: 3600,
            target_architectures: vec![TargetArch::Amd64, TargetArch::Arm64, TargetArch::ArmV7],
            default_target: TargetArch::Arm64,
            builder_images,
            registry: RegistryConfig::default(),
            cache: CacheConfig::default(),
            hooks: BuildHooks::default(),
            resource_limits: BuildResourceLimits::default(),
        }
    }
}

/// Target architectures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetArch {
    Amd64,
    Arm64,
    ArmV7,
    ArmV6,
    RiscV64,
    Ppc64le,
    S390x,
    Mips64le,
    Mips64,
}

impl TargetArch {
    pub fn docker_platform(&self) -> String {
        match self {
            TargetArch::Amd64 => "linux/amd64".to_string(),
            TargetArch::Arm64 => "linux/arm64".to_string(),
            TargetArch::ArmV7 => "linux/arm/v7".to_string(),
            TargetArch::ArmV6 => "linux/arm/v6".to_string(),
            TargetArch::RiscV64 => "linux/riscv64".to_string(),
            TargetArch::Ppc64le => "linux/ppc64le".to_string(),
            TargetArch::S390x => "linux/s390x".to_string(),
            TargetArch::Mips64le => "linux/mips64le".to_string(),
            TargetArch::Mips64 => "linux/mips64".to_string(),
        }
    }

    pub fn rust_target(&self) -> String {
        match self {
            TargetArch::Amd64 => "x86_64-unknown-linux-gnu".to_string(),
            TargetArch::Arm64 => "aarch64-unknown-linux-gnu".to_string(),
            TargetArch::ArmV7 => "armv7-unknown-linux-gnueabihf".to_string(),
            TargetArch::ArmV6 => "arm-unknown-linux-gnueabihf".to_string(),
            TargetArch::RiscV64 => "riscv64gc-unknown-linux-gnu".to_string(),
            TargetArch::Ppc64le => "powerpc64le-unknown-linux-gnu".to_string(),
            TargetArch::S390x => "s390x-unknown-linux-gnu".to_string(),
            TargetArch::Mips64le => "mips64el-unknown-linux-gnuabi64".to_string(),
            TargetArch::Mips64 => "mips64-unknown-linux-gnuabi64".to_string(),
        }
    }

    pub fn go_arch(&self) -> String {
        match self {
            TargetArch::Amd64 => "amd64".to_string(),
            TargetArch::Arm64 => "arm64".to_string(),
            TargetArch::ArmV7 => "arm".to_string(),
            TargetArch::ArmV6 => "arm".to_string(),
            TargetArch::RiscV64 => "riscv64".to_string(),
            TargetArch::Ppc64le => "ppc64le".to_string(),
            TargetArch::S390x => "s390x".to_string(),
            TargetArch::Mips64le => "mips64le".to_string(),
            TargetArch::Mips64 => "mips64".to_string(),
        }
    }
}

impl std::fmt::Display for TargetArch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.docker_platform())
    }
}

/// Registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub insecure: bool,
    pub cert_path: Option<PathBuf>,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            url: "docker.io".to_string(),
            username: None,
            password: None,
            insecure: false,
            cert_path: None,
        }
    }
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub cache_dir: PathBuf,
    pub max_size: u64,
    pub ttl_secs: u64,
    pub registry_cache: bool,
    pub layer_cache: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_dir: PathBuf::from("/var/lib/pdn/build-cache"),
            max_size: 10 * 1024 * 1024 * 1024, // 10GB
            ttl_secs: 86400 * 7,               // 7 days
            registry_cache: true,
            layer_cache: true,
        }
    }
}

/// Build hooks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildHooks {
    pub pre_build: Option<String>,
    pub post_build: Option<String>,
    pub on_success: Option<String>,
    pub on_failure: Option<String>,
    pub pre_push: Option<String>,
    pub post_push: Option<String>,
}

impl Default for BuildHooks {
    fn default() -> Self {
        Self {
            pre_build: None,
            post_build: None,
            on_success: None,
            on_failure: None,
            pre_push: None,
            post_push: None,
        }
    }
}

/// Build resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResourceLimits {
    pub cpu_limit: Option<f64>,
    pub memory_limit: Option<u64>,
    pub disk_limit: Option<u64>,
    pub network_limit: Option<u64>,
    pub timeout_secs: u64,
}

impl Default for BuildResourceLimits {
    fn default() -> Self {
        Self {
            cpu_limit: Some(4.0),
            memory_limit: Some(8 * 1024 * 1024 * 1024), // 8GB
            disk_limit: Some(50 * 1024 * 1024 * 1024),  // 50GB
            network_limit: None,
            timeout_secs: 3600,
        }
    }
}

/// Build status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildStatus {
    Pending,
    Queued,
    Preparing,
    Building,
    Testing,
    Pushing,
    Completed,
    Failed,
    Cancelled,
}

/// Build specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildSpec {
    pub id: String,
    pub name: String,
    pub source: BuildSource,
    pub targets: Vec<TargetArch>,
    pub dockerfile: Option<String>,
    pub context: String,
    pub args: HashMap<String, String>,
    pub labels: HashMap<String, String>,
    pub cache_from: Vec<String>,
    pub cache_to: Vec<String>,
    pub secrets: Vec<BuildSecret>,
    pub ssh_keys: Vec<String>,
    pub network_mode: String,
    pub platform: Option<String>,
    pub output: BuildOutput,
    pub annotations: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildSource {
    Git {
        url: String,
        ref_: String,
        subdir: Option<String>,
    },
    Local {
        path: PathBuf,
    },
    Tarball {
        url: String,
    },
    Scratch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildSecret {
    pub id: String,
    pub source: SecretSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretSource {
    Env { name: String },
    File { path: PathBuf },
    Literal { value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildOutput {
    pub type_: OutputType,
    pub registry: Option<String>,
    pub repository: String,
    pub tag: String,
    pub digest_file: Option<PathBuf>,
    pub oci_mediatypes: bool,
    pub push: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    Image,
    Registry,
    Local,
    Tar,
    Oci,
}

/// Build metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildMetadata {
    pub id: String,
    pub name: String,
    pub spec: BuildSpec,
    pub status: BuildStatus,
    pub created_at: SystemTime,
    pub started_at: Option<SystemTime>,
    pub completed_at: Option<SystemTime>,
    pub progress: BuildProgress,
    pub logs: Vec<LogEntry>,
    pub artifacts: Vec<BuildArtifact>,
    pub error_message: Option<String>,
    pub builder_node: Option<String>,
    pub platform_results: HashMap<TargetArch, PlatformResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildProgress {
    pub current_stage: String,
    pub stage_progress: f64,
    pub overall_progress: f64,
    pub current_target: Option<TargetArch>,
    pub targets_completed: usize,
    pub targets_total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: SystemTime,
    pub level: String,
    pub message: String,
    pub target: Option<TargetArch>,
    pub stage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildArtifact {
    pub id: String,
    pub target: TargetArch,
    pub digest: String,
    pub size: u64,
    pub path: PathBuf,
    pub mediatype: String,
    pub annotations: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformResult {
    pub target: TargetArch,
    pub status: BuildStatus,
    pub digest: Option<String>,
    pub size: u64,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub layers: Vec<LayerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerInfo {
    pub digest: String,
    pub size: u64,
    pub instruction: String,
    pub cache_hit: bool,
}

/// Build events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuildEvent {
    BuildQueued {
        build_id: String,
        name: String,
    },
    BuildStarted {
        build_id: String,
        target: TargetArch,
    },
    BuildStageStarted {
        build_id: String,
        target: TargetArch,
        stage: String,
    },
    BuildProgress {
        build_id: String,
        target: TargetArch,
        progress: f64,
    },
    BuildCompleted {
        build_id: String,
        target: TargetArch,
        digest: String,
        size: u64,
    },
    BuildFailed {
        build_id: String,
        target: TargetArch,
        error: String,
    },
    BuildCancelled {
        build_id: String,
    },
    ArtifactCreated {
        build_id: String,
        target: TargetArch,
        artifact: BuildArtifact,
    },
    CacheHit {
        build_id: String,
        target: TargetArch,
        layer: String,
    },
    CacheMiss {
        build_id: String,
        target: TargetArch,
        layer: String,
    },
}

/// Build manager
pub struct BuildManager {
    config: BuildServiceConfig,
    builds: Arc<RwLock<HashMap<String, BuildMetadata>>>,
    active_builds: Arc<Mutex<HashMap<String, BuildHandle>>>,
    build_queue: Arc<Mutex<Vec<BuildSpec>>>,
    event_sender: broadcast::Sender<BuildEvent>,
    cancel_tokens: Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<()>>>>,
    semaphore: Arc<tokio::sync::Semaphore>,
}

#[derive(Debug)]
struct BuildHandle {
    build_id: String,
    cancel_tx: tokio::sync::oneshot::Sender<()>,
    started_at: SystemTime,
    current_target: Option<TargetArch>,
}

impl BuildManager {
    pub fn new(config: BuildServiceConfig) -> Self {
        let (event_sender, _) = broadcast::channel(1000);
        let semaphore = Arc::new(tokio::sync::Semaphore::new(config.max_concurrent_builds));

        // Ensure directories exist
        std::fs::create_dir_all(&config.workspace_dir).ok();
        std::fs::create_dir_all(&config.artifact_dir).ok();
        if config.cache.enabled {
            std::fs::create_dir_all(&config.cache.cache_dir).ok();
        }

        Self {
            config,
            builds: Arc::new(RwLock::new(HashMap::new())),
            active_builds: Arc::new(Mutex::new(HashMap::new())),
            build_queue: Arc::new(Mutex::new(Vec::new())),
            event_sender,
            cancel_tokens: Arc::new(RwLock::new(HashMap::new())),
            semaphore,
        }
    }

    pub async fn start(&self) -> Result<()> {
        if !self.config.enabled {
            info!("Build service disabled");
            return Ok(());
        }

        info!("Starting build service");

        // Start build processor
        self.start_build_processor().await;

        // Start cleanup task
        self.start_cleanup_task().await;

        Ok(())
    }

    pub async fn submit_build(&self, spec: BuildSpec) -> Result<String> {
        let build_id = spec.id.clone();
        let build_name = spec.name.clone();
        let now = SystemTime::now();

        let metadata = BuildMetadata {
            id: build_id.clone(),
            name: spec.name.clone(),
            spec: spec.clone(),
            status: BuildStatus::Queued,
            created_at: now,
            started_at: None,
            completed_at: None,
            progress: BuildProgress {
                current_stage: "Queued".to_string(),
                stage_progress: 0.0,
                overall_progress: 0.0,
                current_target: None,
                targets_completed: 0,
                targets_total: spec.targets.len(),
            },
            logs: Vec::new(),
            artifacts: Vec::new(),
            error_message: None,
            builder_node: None,
            platform_results: spec
                .targets
                .iter()
                .map(|t| {
                    (
                        *t,
                        PlatformResult {
                            target: *t,
                            status: BuildStatus::Pending,
                            digest: None,
                            size: 0,
                            duration_ms: 0,
                            error: None,
                            layers: Vec::new(),
                        },
                    )
                })
                .collect(),
        };

        {
            let mut builds = self.builds.write().await;
            builds.insert(build_id.clone(), metadata);
        }

        {
            let mut queue = self.build_queue.lock().await;
            queue.push(spec);
        }

        self.emit_event(BuildEvent::BuildQueued {
            build_id: build_id.clone(),
            name: build_name,
        });

        Ok(build_id)
    }

    async fn start_build_processor(&self) {
        let manager = self.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));

            loop {
                interval.tick().await;

                // Process queue
                let spec = {
                    let mut queue = manager.build_queue.lock().await;
                    queue.pop()
                };

                if let Some(spec) = spec {
                    let permit = manager.semaphore.clone().acquire_owned().await.ok();

                    let manager_clone = manager.clone();
                    let spec_clone = spec.clone();

                    tokio::spawn(async move {
                        let _permit = permit;
                        if let Err(e) = manager_clone.run_build(spec_clone).await {
                            error!("Build failed: {}", e);
                        }
                    });
                }
            }
        });
    }

    async fn run_build(&self, spec: BuildSpec) -> Result<()> {
        let build_id = spec.id.clone();
        let start_time = Instant::now();

        // Update status to preparing
        self.update_build_status(&build_id, BuildStatus::Preparing)
            .await;

        // Create cancel token (using AtomicBool for shared access)
        let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel_flag_clone = cancel_flag.clone();
        {
            let mut tokens = self.cancel_tokens.write().await;
            // Store a oneshot sender for external cancellation, but use AtomicBool internally
            let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
            tokens.insert(build_id.clone(), cancel_tx);
            // Spawn a task to forward oneshot cancellation to the atomic flag
            tokio::spawn(async move {
                cancel_rx.await.ok();
                cancel_flag_clone.store(true, std::sync::atomic::Ordering::Relaxed);
            });
        }

        // Track active build
        {
            let mut active = self.active_builds.lock().await;
            active.insert(
                build_id.clone(),
                BuildHandle {
                    build_id: build_id.clone(),
                    cancel_tx: tokio::sync::oneshot::channel().0, // placeholder
                    started_at: SystemTime::now(),
                    current_target: None,
                },
            );
        }

        // Run pre-build hook
        if let Some(hook) = &self.config.hooks.pre_build {
            self.run_hook(&hook, &build_id).await.ok();
        }

        let mut overall_success = true;
        let mut first_error = None;

        // Build for each target architecture
        for (idx, target) in spec.targets.iter().copied().enumerate() {
            // Check cancellation
            if cancel_flag.load(std::sync::atomic::Ordering::Relaxed) {
                self.update_build_status(&build_id, BuildStatus::Cancelled)
                    .await;
                return Ok(());
            }

            self.update_build_progress(
                &build_id,
                BuildProgress {
                    current_stage: format!("Building for {}", target),
                    stage_progress: 0.0,
                    overall_progress: (idx as f64 / spec.targets.len() as f64) * 100.0,
                    current_target: Some(target),
                    targets_completed: idx,
                    targets_total: spec.targets.len(),
                },
            )
            .await;

            self.emit_event(BuildEvent::BuildStarted {
                build_id: build_id.clone(),
                target,
            });

            // Update platform result
            self.update_platform_result(
                &build_id,
                target,
                PlatformResult {
                    target,
                    status: BuildStatus::Building,
                    digest: None,
                    size: 0,
                    duration_ms: 0,
                    error: None,
                    layers: Vec::new(),
                },
            )
            .await;

            let target_start = Instant::now();

            // Run build for target
            let result = self
                .build_for_target(&build_id, &spec, target, cancel_flag.clone())
                .await;

            let duration = target_start.elapsed().as_millis() as u64;

            match result {
                Ok((digest, size, layers)) => {
                    self.update_platform_result(
                        &build_id,
                        target,
                        PlatformResult {
                            target,
                            status: BuildStatus::Completed,
                            digest: Some(digest.clone()),
                            size,
                            duration_ms: duration,
                            error: None,
                            layers,
                        },
                    )
                    .await;

                    self.emit_event(BuildEvent::BuildCompleted {
                        build_id: build_id.clone(),
                        target,
                        digest: digest.clone(),
                        size,
                    });
                }
                Err(e) => {
                    overall_success = false;
                    first_error = Some(e.to_string());

                    self.update_platform_result(
                        &build_id,
                        target,
                        PlatformResult {
                            target,
                            status: BuildStatus::Failed,
                            digest: None,
                            size: 0,
                            duration_ms: duration,
                            error: Some(e.to_string()),
                            layers: Vec::new(),
                        },
                    )
                    .await;

                    self.emit_event(BuildEvent::BuildFailed {
                        build_id: build_id.clone(),
                        target,
                        error: e.to_string(),
                    });

                    break;
                }
            }

            // Finalize
            if overall_success {
                self.update_build_status(&build_id, BuildStatus::Completed)
                    .await;

                // Run post-build hook
                if let Some(hook) = &self.config.hooks.on_success {
                    self.run_hook(&hook, &build_id).await.ok();
                }
            } else {
                self.update_build_status(&build_id, BuildStatus::Failed)
                    .await;
                if let Some(ref error) = first_error {
                    self.update_build_error(&build_id, error.clone()).await;
                }

                if let Some(hook) = &self.config.hooks.on_failure {
                    self.run_hook(&hook, &build_id).await.ok();
                }
            }
        }

        // Cleanup
        self.active_builds.lock().await.remove(&build_id);
        self.cancel_tokens.write().await.remove(&build_id);

        Ok(())
    }

    async fn build_for_target(
        &self,
        build_id: &str,
        spec: &BuildSpec,
        target: TargetArch,
        cancel_flag: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(String, u64, Vec<LayerInfo>)> {
        // Update stage: preparing
        self.add_log(build_id, target, "info", "Preparing build environment")
            .await;

        // Prepare build context
        let context_dir = self.prepare_context(spec).await?;

        // Update stage: building
        self.add_log(build_id, target, "info", "Starting build")
            .await;
        self.update_stage_progress(build_id, target, "Building", 0.5)
            .await;

        // Run build using appropriate builder
        let builder_image = self
            .config
            .builder_images
            .get(&target)
            .context("No builder image for target")?;

        // In real implementation, would run docker buildx or similar
        // For now, simulate build
        self.add_log(
            build_id,
            target,
            "info",
            &format!("Building for {}", target),
        )
        .await;

        // Simulate build stages
        for i in 1..=10 {
            self.update_stage_progress(build_id, target, "Building", i as f64 / 10.0)
                .await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Update stage: testing
        self.add_log(build_id, target, "info", "Running tests")
            .await;
        self.update_stage_progress(build_id, target, "Testing", 0.0)
            .await;

        // Update stage: pushing (if needed)
        if spec.output.push {
            self.add_log(build_id, target, "info", "Pushing to registry")
                .await;
            self.update_stage_progress(build_id, target, "Pushing", 0.0)
                .await;
        }

        // Simulated result
        let digest = format!("sha256:{}", Uuid::new_v4().to_string().replace('-', ""));
        let size = 100 * 1024 * 1024; // 100MB
        let layers = vec![
            LayerInfo {
                digest: format!("sha256:{}", Uuid::new_v4().to_string().replace('-', "")),
                size: 50 * 1024 * 1024,
                instruction: "FROM base".to_string(),
                cache_hit: false,
            },
            LayerInfo {
                digest: format!("sha256:{}", Uuid::new_v4().to_string().replace('-', "")),
                size: 30 * 1024 * 1024,
                instruction: "COPY . .".to_string(),
                cache_hit: false,
            },
            LayerInfo {
                digest: format!("sha256:{}", Uuid::new_v4().to_string().replace('-', "")),
                size: 20 * 1024 * 1024,
                instruction: "RUN build".to_string(),
                cache_hit: false,
            },
        ];

        Ok((digest, size, layers))
    }

    async fn prepare_context(&self, spec: &BuildSpec) -> Result<PathBuf> {
        let context_dir = self.config.workspace_dir.join(&spec.id);
        fs::create_dir_all(&context_dir).await?;

        match &spec.source {
            BuildSource::Git { url, ref_, subdir } => {
                // Clone repository
                let repo_dir = context_dir.join("source");
                // Would use git2 to clone
                info!("Cloning {} at {}", url, ref_);
            }
            BuildSource::Local { path } => {
                // Copy local context
                fs::copy(path, context_dir.join("source")).await?;
            }
            BuildSource::Tarball { url } => {
                // Download and extract
            }
            BuildSource::Scratch => {}
        }

        Ok(context_dir)
    }

    async fn run_hook(&self, hook: &str, build_id: &str) -> Result<()> {
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(hook)
            .env("BUILD_ID", build_id)
            .output()
            .await?;

        if !output.status.success() {
            warn!("Hook failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        Ok(())
    }

    async fn add_log(&self, build_id: &str, target: TargetArch, level: &str, message: &str) {
        let mut builds = self.builds.write().await;
        if let Some(build) = builds.get_mut(build_id) {
            build.logs.push(LogEntry {
                timestamp: SystemTime::now(),
                level: level.to_string(),
                message: message.to_string(),
                target: Some(target),
                stage: build.progress.current_stage.clone(),
            });
        }
    }

    async fn update_stage_progress(
        &self,
        build_id: &str,
        target: TargetArch,
        stage: &str,
        progress: f64,
    ) {
        let mut builds = self.builds.write().await;
        if let Some(build) = builds.get_mut(build_id) {
            build.progress.current_stage = stage.to_string();
            build.progress.stage_progress = progress;
            build.progress.current_target = Some(target);
        }
    }

    async fn update_build_status(&self, build_id: &str, status: BuildStatus) {
        let mut builds = self.builds.write().await;
        if let Some(build) = builds.get_mut(build_id) {
            build.status = status;
            if status == BuildStatus::Preparing {
                build.started_at = Some(SystemTime::now());
            } else if matches!(
                status,
                BuildStatus::Completed | BuildStatus::Failed | BuildStatus::Cancelled
            ) {
                build.completed_at = Some(SystemTime::now());
            }
        }
    }

    async fn update_build_progress(&self, build_id: &str, progress: BuildProgress) {
        let mut builds = self.builds.write().await;
        if let Some(build) = builds.get_mut(build_id) {
            build.progress = progress;
        }
    }

    async fn update_platform_result(
        &self,
        build_id: &str,
        target: TargetArch,
        result: PlatformResult,
    ) {
        let mut builds = self.builds.write().await;
        if let Some(build) = builds.get_mut(build_id) {
            build.platform_results.insert(target, result);
        }
    }

    async fn update_build_error(&self, build_id: &str, error: String) {
        let mut builds = self.builds.write().await;
        if let Some(build) = builds.get_mut(build_id) {
            build.error_message = Some(error);
        }
    }

    pub async fn get_build(&self, build_id: &str) -> Option<BuildMetadata> {
        self.builds.read().await.get(build_id).cloned()
    }

    pub async fn list_builds(&self, status: Option<BuildStatus>) -> Vec<BuildMetadata> {
        let builds = self.builds.read().await;
        builds
            .values()
            .filter(|b| status.map_or(true, |s| b.status == s))
            .cloned()
            .collect()
    }

    pub async fn cancel_build(&self, build_id: &str) -> Result<()> {
        let mut tokens = self.cancel_tokens.write().await;
        if let Some(tx) = tokens.remove(build_id) {
            tx.send(()).ok();
        }

        self.update_build_status(build_id, BuildStatus::Cancelled)
            .await;

        self.emit_event(BuildEvent::BuildCancelled {
            build_id: build_id.to_string(),
        });

        Ok(())
    }

    pub async fn retry_build(&self, build_id: &str) -> Result<String> {
        let spec = {
            let builds = self.builds.read().await;
            builds.get(build_id).map(|b| b.spec.clone())
        }
        .context("Build not found")?;

        let new_spec = BuildSpec {
            id: Uuid::new_v4().to_string(),
            name: format!("{}-retry", spec.name),
            ..spec
        };

        self.submit_build(new_spec).await
    }

    pub async fn get_build_logs(
        &self,
        build_id: &str,
        target: Option<TargetArch>,
    ) -> Vec<LogEntry> {
        let builds = self.builds.read().await;
        builds
            .get(build_id)
            .map(|b| {
                b.logs
                    .iter()
                    .filter(|l| target.map_or(true, |t| l.target == Some(t)))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn get_artifacts(&self, build_id: &str) -> Vec<BuildArtifact> {
        let builds = self.builds.read().await;
        builds
            .get(build_id)
            .map(|b| b.artifacts.clone())
            .unwrap_or_default()
    }

    async fn start_cleanup_task(&self) {
        let config = self.config.clone();
        let manager = self.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(3600)); // Every hour

            loop {
                interval.tick().await;
                manager.cleanup_old_builds().await.ok();
            }
        });
    }

    async fn cleanup_old_builds(&self) -> Result<()> {
        let builds = self.builds.read().await;
        let now = SystemTime::now();
        let max_age = Duration::from_secs(86400 * 30); // 30 days

        let to_remove: Vec<String> = builds
            .values()
            .filter(|b| {
                matches!(
                    b.status,
                    BuildStatus::Completed | BuildStatus::Failed | BuildStatus::Cancelled
                ) && now.duration_since(b.created_at).unwrap_or_default() > max_age
            })
            .map(|b| b.id.clone())
            .collect();

        drop(builds);

        for id in to_remove {
            let mut builds = self.builds.write().await;
            if let Some(build) = builds.remove(&id) {
                // Clean up artifacts
                for artifact in &build.artifacts {
                    fs::remove_file(&artifact.path).await.ok();
                }
            }
        }

        Ok(())
    }

    fn emit_event(&self, event: BuildEvent) {
        self.event_sender.send(event).ok();
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<BuildEvent> {
        self.event_sender.subscribe()
    }

    pub fn get_config(&self) -> &BuildServiceConfig {
        &self.config
    }
}

impl Clone for BuildManager {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            builds: self.builds.clone(),
            active_builds: self.active_builds.clone(),
            build_queue: self.build_queue.clone(),
            event_sender: self.event_sender.clone(),
            cancel_tokens: self.cancel_tokens.clone(),
            semaphore: self.semaphore.clone(),
        }
    }
}

impl Default for PlatformResult {
    fn default() -> Self {
        Self {
            target: TargetArch::Arm64,
            status: BuildStatus::Pending,
            digest: None,
            size: 0,
            duration_ms: 0,
            error: None,
            layers: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_build_config() {
        let config = BuildServiceConfig::default();
        assert!(config.enabled);
        assert!(config.target_architectures.contains(&TargetArch::Arm64));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_target_arch() {
        assert_eq!(TargetArch::Arm64.docker_platform(), "linux/arm64");
        assert_eq!(TargetArch::Amd64.rust_target(), "x86_64-unknown-linux-gnu");
    }
}
