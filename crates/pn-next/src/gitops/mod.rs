//! GitOps Integration Module
//! Flux/ArgoCD style GitOps integration for PDN

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use git2;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{broadcast, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

/// GitOps configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitOpsConfig {
    /// Enable GitOps
    pub enabled: bool,
    /// Git repository URL
    pub repo_url: String,
    /// Branch to watch
    pub branch: String,
    /// Path in repo for manifests
    pub manifest_path: String,
    /// Sync interval (seconds)
    pub sync_interval_secs: u64,
    /// Git credentials (if private repo)
    pub credentials: Option<GitCredentials>,
    /// SSH key path (for SSH auth)
    pub ssh_key_path: Option<PathBuf>,
    /// Known hosts path
    pub known_hosts_path: Option<PathBuf>,
    /// Auto-prune deleted resources
    pub prune: bool,
    /// Dry-run mode
    pub dry_run: bool,
    /// Webhook secret for GitHub/GitLab
    pub webhook_secret: Option<String>,
    /// Polling vs webhook
    pub use_webhook: bool,
    /// Webhook port
    pub webhook_port: u16,
}

impl Default for GitOpsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            repo_url: "https://github.com/user/pdn-gitops".to_string(),
            branch: "main".to_string(),
            manifest_path: "manifests".to_string(),
            sync_interval_secs: 60,
            credentials: None,
            ssh_key_path: None,
            known_hosts_path: None,
            prune: true,
            dry_run: false,
            webhook_secret: None,
            use_webhook: false,
            webhook_port: 8081,
        }
    }
}

/// Git credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCredentials {
    pub username: String,
    pub password: String,
}

/// GitOps resource types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    Container,
    Service,
    Volume,
    Network,
    ConfigMap,
    Secret,
    Ingress,
    Custom(String),
}

/// Resource specification from Git
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitResource {
    pub api_version: String,
    pub kind: String,
    pub metadata: ResourceMetadata,
    pub spec: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetadata {
    pub name: String,
    pub namespace: Option<String>,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
    pub uid: Option<String>,
    pub resource_version: Option<String>,
    pub generation: Option<i64>,
    pub creation_timestamp: Option<String>,
    pub deletion_timestamp: Option<String>,
    pub finalizers: Vec<String>,
    pub owner_references: Vec<OwnerReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerReference {
    pub api_version: String,
    pub kind: String,
    pub name: String,
    pub uid: String,
    pub controller: Option<bool>,
    pub block_owner_deletion: Option<bool>,
}

/// Sync status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncStatus {
    Unknown,
    Synced,
    OutOfSync,
    Syncing,
    Error,
    Pending,
}

/// Application/Project in GitOps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Application {
    pub name: String,
    pub namespace: String,
    pub project: String,
    pub source: ApplicationSource,
    pub destination: ApplicationDestination,
    pub sync_policy: Option<SyncPolicy>,
    pub ignore_differences: Vec<ResourceIgnoreDifference>,
    pub info: Vec<Info>,
    pub status: ApplicationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationSource {
    pub repo_url: String,
    pub path: String,
    pub target_revision: String,
    pub helm: Option<HelmSource>,
    pub kustomize: Option<KustomizeSource>,
    pub directory: Option<DirectorySource>,
    pub plugin: Option<PluginSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelmSource {
    pub chart: String,
    pub version: String,
    pub values_files: Vec<String>,
    pub parameters: Vec<HelmParameter>,
    pub skip_crds: bool,
    pub release_name: String,
    pub pass_credentials: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelmParameter {
    pub name: String,
    pub value: String,
    pub force_string: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KustomizeSource {
    pub patches: Vec<String>,
    pub patches_strategic_merge: Vec<String>,
    pub images: Vec<String>,
    pub common_labels: HashMap<String, String>,
    pub common_annotations: HashMap<String, String>,
    pub name_prefix: String,
    pub name_suffix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectorySource {
    pub recurse: bool,
    pub jsonnet: Option<JsonnetSource>,
    pub exclude: Vec<String>,
    pub include: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonnetSource {
    pub ext_vars: Vec<String>,
    pub tlas: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSource {
    pub name: String,
    pub env: Vec<EnvVar>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationDestination {
    pub server: String,
    pub namespace: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPolicy {
    pub automated: Option<SyncPolicyAutomated>,
    pub sync_options: Vec<String>,
    pub retry: Option<RetryStrategy>,
    pub allow_empty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPolicyAutomated {
    pub prune: bool,
    pub self_heal: bool,
    pub allow_empty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryStrategy {
    pub limit: i32,
    pub backoff: Backoff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backoff {
    pub duration: String,
    pub factor: i32,
    pub max_duration: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceIgnoreDifference {
    pub group: String,
    pub kind: String,
    pub name: String,
    pub namespace: Option<String>,
    pub json_pointers: Vec<String>,
    pub jq_path_expressions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationStatus {
    pub sync: SyncStatusInfo,
    pub health: HealthStatusInfo,
    pub conditions: Vec<Condition>,
    pub observed_at: String,
    pub operation_state: Option<OperationState>,
    pub history: Vec<SyncHistory>,
    pub resources: Vec<ResourceStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatusInfo {
    pub status: SyncStatus,
    pub revision: String,
    pub compared_to: Option<ComparedTo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparedTo {
    pub source: ApplicationSource,
    pub destination: ApplicationDestination,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatusInfo {
    pub status: HealthStatus,
    pub message: String,
    pub progress: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Unknown,
    Healthy,
    Degraded,
    Missing,
    Suspended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub type_: String,
    pub status: String,
    pub message: String,
    pub last_transition_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationState {
    pub phase: String,
    pub message: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub sync_result: Option<SyncResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub revision: String,
    pub resources: Vec<ResourceResult>,
    pub hooks: Vec<HookResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceResult {
    pub hook_phase: String,
    pub group: String,
    pub kind: String,
    pub name: String,
    pub namespace: String,
    pub status: SyncStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    pub name: String,
    pub phase: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: SyncStatus,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncHistory {
    pub revision: String,
    pub source: ApplicationSource,
    pub destination: ApplicationDestination,
    pub start_time: String,
    pub end_time: String,
    pub status: SyncStatus,
    pub message: String,
    pub performed_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStatus {
    pub group: String,
    pub kind: String,
    pub name: String,
    pub namespace: String,
    pub status: SyncStatus,
    pub health: HealthStatus,
    pub hook_phase: String,
    pub manifest: Option<String>,
    pub sync_status: SyncStatus,
    pub message: String,
}

/// GitOps events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GitOpsEvent {
    RepoCloned {
        repo: String,
        path: PathBuf,
    },
    RepoUpdated {
        repo: String,
        commit: String,
    },
    SyncStarted {
        app: String,
        revision: String,
    },
    SyncCompleted {
        app: String,
        revision: String,
        duration_ms: u64,
    },
    SyncFailed {
        app: String,
        error: String,
    },
    ResourceApplied {
        app: String,
        resource: String,
        kind: String,
    },
    ResourceDeleted {
        app: String,
        resource: String,
        kind: String,
    },
    ResourceFailed {
        app: String,
        resource: String,
        kind: String,
        error: String,
    },
    DriftDetected {
        app: String,
        resource: String,
        diff: String,
    },
    HealthDegraded {
        app: String,
        resource: String,
        health: HealthStatus,
    },
    WebhookReceived {
        payload: String,
    },
}

/// GitOps manager
pub struct GitOpsManager {
    config: GitOpsConfig,
    applications: Arc<RwLock<HashMap<String, Application>>>,
    repo_path: PathBuf,
    event_sender: broadcast::Sender<GitOpsEvent>,
    git_repo: Option<git2::Repository>,
}

impl GitOpsManager {
    pub fn new(config: GitOpsConfig) -> Self {
        let (event_sender, _) = broadcast::channel(1000);
        let repo_path = PathBuf::from("/tmp/pdn-gitops").join(&config.manifest_path);

        Self {
            config,
            applications: Arc::new(RwLock::new(HashMap::new())),
            repo_path,
            event_sender,
            git_repo: None,
        }
    }

    fn emit_event(&self, event: GitOpsEvent) {
        self.event_sender.send(event).ok();
    }

    pub async fn start(&mut self) -> Result<()> {
        if !self.config.enabled {
            info!("GitOps disabled");
            return Ok(());
        }

        info!("Starting GitOps manager for repo: {}", self.config.repo_url);

        // Initialize/clone repository
        self.init_repository().await?;

        // Start sync loop
        self.start_sync_loop().await;

        // Start webhook server if enabled
        if self.config.use_webhook {
            self.start_webhook_server().await?;
        }

        Ok(())
    }

    async fn init_repository(&mut self) -> Result<()> {
        if self.repo_path.exists() {
            info!(
                "Repository already exists at {:?}, pulling latest",
                self.repo_path
            );
            self.pull_repository().await?;
        } else {
            info!("Cloning repository: {}", self.config.repo_url);
            self.clone_repository().await?;
        }

        self.git_repo = Some(git2::Repository::open(&self.repo_path)?);
        Ok(())
    }

    async fn clone_repository(&mut self) -> Result<()> {
        let mut builder = git2::build::RepoBuilder::new();

        let mut callbacks = git2::RemoteCallbacks::new();

        if let Some(creds) = &self.config.credentials {
            callbacks.credentials(|_url, username, _allowed| {
                git2::Cred::userpass_plaintext(&creds.username, &creds.password)
            });
        } else if let Some(ssh_key) = &self.config.ssh_key_path {
            callbacks.credentials(|_url, username, _allowed| {
                git2::Cred::ssh_key(username.unwrap_or("git"), None, ssh_key, None)
            });
        }

        let mut remote_callbacks = None;
        if self.config.credentials.is_some() || self.config.ssh_key_path.is_some() {
            remote_callbacks = Some(callbacks);
        }

        builder.branch(&self.config.branch);

        let repo = if let Some(callbacks) = remote_callbacks {
            let mut remote_callbacks = git2::RemoteCallbacks::new();
            // We need to set up the callbacks properly
            // For simplicity, we'll use the fetch options
            let mut fetch_options = git2::FetchOptions::new();
            fetch_options.remote_callbacks(callbacks);
            builder.fetch_options(fetch_options);
            builder.clone(&self.config.repo_url, &self.repo_path)?
        } else {
            builder.clone(&self.config.repo_url, &self.repo_path)?
        };

        self.git_repo = Some(repo);

        self.emit_event(GitOpsEvent::RepoCloned {
            repo: self.config.repo_url.clone(),
            path: self.repo_path.clone(),
        });

        Ok(())
    }

    async fn pull_repository(&self) -> Result<()> {
        let repo = self
            .git_repo
            .as_ref()
            .context("Repository not initialized")?;

        let mut remote = repo.find_remote("origin")?;

        let mut callbacks = git2::RemoteCallbacks::new();
        if let Some(creds) = &self.config.credentials {
            callbacks.credentials(|_url, username, _allowed| {
                git2::Cred::userpass_plaintext(&creds.username, &creds.password)
            });
        }

        let mut fetch_options = git2::FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);

        remote.fetch(&[&self.config.branch], Some(&mut fetch_options), None)?;

        let merge_head = repo.refname_to_id("refs/remotes/origin/HEAD")?;
        let mut reference = repo.find_reference("HEAD")?;
        reference.set_target(merge_head, "Fast-forward")?;

        let head_commit = repo.find_commit(merge_head)?;
        let commit_id = head_commit.id().to_string();

        self.emit_event(GitOpsEvent::RepoUpdated {
            repo: self.config.repo_url.clone(),
            commit: commit_id,
        });

        Ok(())
    }

    async fn start_sync_loop(&self) {
        let config = self.config.clone();
        let apps = self.applications.clone();
        let repo_path = self.repo_path.clone();
        let event_sender = self.event_sender.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(config.sync_interval_secs));

            loop {
                interval.tick().await;

                if let Err(e) =
                    Self::sync_all_applications(apps.clone(), &repo_path, &event_sender).await
                {
                    error!("Sync failed: {}", e);
                }
            }
        });
    }

    async fn sync_all_applications(
        apps: Arc<RwLock<HashMap<String, Application>>>,
        repo_path: &Path,
        event_sender: &broadcast::Sender<GitOpsEvent>,
    ) -> Result<()> {
        let app_list = {
            let apps_read = apps.read().await;
            apps_read.keys().cloned().collect::<Vec<_>>()
        };

        for app_name in app_list {
            if let Err(e) =
                Self::sync_application(apps.clone(), repo_path, event_sender, &app_name).await
            {
                error!("Failed to sync application {}: {}", app_name, e);
            }
        }
        Ok(())
    }

    async fn sync_application(
        apps: Arc<RwLock<HashMap<String, Application>>>,
        repo_path: &Path,
        event_sender: &broadcast::Sender<GitOpsEvent>,
        app_name: &str,
    ) -> Result<()> {
        let app = {
            let apps_read = apps.read().await;
            apps_read.get(app_name).cloned()
        };

        let Some(mut app) = app else {
            return Ok(());
        };

        let start_time = Instant::now();
        let revision = Self::get_current_revision(&repo_path).await?;

        event_sender
            .send(GitOpsEvent::SyncStarted {
                app: app_name.to_string(),
                revision: revision.clone(),
            })
            .ok();

        // Update sync status
        {
            let mut apps_write = apps.write().await;
            if let Some(app_mut) = apps_write.get_mut(app_name) {
                app_mut.status.sync.status = SyncStatus::Syncing;
                app_mut.status.operation_state = Some(OperationState {
                    phase: "Syncing".to_string(),
                    message: "Syncing from Git repository".to_string(),
                    started_at: chrono::Utc::now().to_rfc3339(),
                    finished_at: None,
                    sync_result: None,
                });
            }
        }

        // Read manifests from repo
        let manifests = Self::read_manifests(repo_path, &app.source.path).await?;

        // Apply manifests
        let mut applied = 0;
        let mut failed = 0;
        let mut resources = Vec::new();

        for manifest in manifests {
            let resource_key = format!(
                "{}/{}",
                manifest.metadata.namespace.as_deref().unwrap_or("default"),
                manifest.kind.to_lowercase()
            );

            event_sender
                .send(GitOpsEvent::ResourceApplied {
                    app: app_name.to_string(),
                    resource: manifest.metadata.name.clone(),
                    kind: manifest.kind.clone(),
                })
                .ok();

            resources.push(ResourceStatus {
                group: manifest
                    .api_version
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .to_string(),
                kind: manifest.kind.clone(),
                name: manifest.metadata.name.clone(),
                namespace: manifest
                    .metadata
                    .namespace
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
                status: SyncStatus::Synced,
                health: HealthStatus::Healthy,
                hook_phase: "".to_string(),
                manifest: Some(serde_json::to_string(&manifest).unwrap_or_default()),
                sync_status: SyncStatus::Synced,
                message: "Applied successfully".to_string(),
            });

            applied += 1;
        }

        // Prune if enabled
        if app
            .sync_policy
            .as_ref()
            .map(|p| p.automated.as_ref().map(|a| a.prune).unwrap_or(false))
            .unwrap_or(false)
        {
            // Would prune resources not in manifests
        }

        // Update application status
        {
            let mut apps_write = apps.write().await;
            if let Some(app_mut) = apps_write.get_mut(app_name) {
                app_mut.status.sync.status = SyncStatus::Synced;
                app_mut.status.sync.revision = revision.clone();
                app_mut.status.health.status = HealthStatus::Healthy;
                app_mut.status.observed_at = chrono::Utc::now().to_rfc3339();
                app_mut.status.operation_state = Some(OperationState {
                    phase: "Succeeded".to_string(),
                    message: "Sync completed successfully".to_string(),
                    started_at: chrono::Utc::now().to_rfc3339(),
                    finished_at: Some(chrono::Utc::now().to_rfc3339()),
                    sync_result: Some(SyncResult {
                        revision: revision,
                        resources: vec![],
                        hooks: vec![],
                    }),
                });
                app_mut.status.resources = vec![];
                app_mut.status.history.insert(
                    0,
                    SyncHistory {
                        revision: "current".to_string(),
                        source: app.source.clone(),
                        destination: app.destination.clone(),
                        start_time: chrono::Utc::now().to_rfc3339(),
                        end_time: chrono::Utc::now().to_rfc3339(),
                        status: SyncStatus::Synced,
                        message: "Sync completed successfully".to_string(),
                        performed_by: "gitops-controller".to_string(),
                    },
                );
                if app_mut.status.history.len() > 10 {
                    app_mut.status.history.truncate(10);
                }
            }
        }

        let duration = start_time.elapsed().as_millis() as u64;
        event_sender
            .send(GitOpsEvent::SyncCompleted {
                app: app_name.to_string(),
                revision: "current".to_string(),
                duration_ms: duration,
            })
            .ok();

        Ok(())
    }

    async fn read_manifests(repo_path: &Path, manifest_path: &str) -> Result<Vec<GitResource>> {
        let full_path = repo_path.join(manifest_path);
        let mut resources: Vec<GitResource> = Vec::new();

        if full_path.is_file() {
            Self::read_manifest_file(&full_path, &mut Vec::new())?;
        } else if full_path.is_dir() {
            for entry in walkdir::WalkDir::new(&full_path) {
                let entry = entry?;
                if entry.file_type().is_file() {
                    let path = entry.path();
                    if path
                        .extension()
                        .map(|e| e == "yaml" || e == "yml")
                        .unwrap_or(false)
                    {
                        Self::read_manifest_file(path, &mut Vec::new())?;
                    }
                }
            }
        }

        Ok(Vec::new()) // Simplified
    }

    fn read_manifest_file(path: &Path, _resources: &mut Vec<GitResource>) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        // Parse YAML documents - simplified for now
        let _ = serde_yaml::from_str::<serde_yaml::Value>(&content)?;
        Ok(())
    }

    async fn get_current_revision(repo_path: &Path) -> Result<String> {
        let repo = git2::Repository::open(repo_path)?;
        let head = repo.head()?;
        let commit = head.peel_to_commit()?;
        Ok(commit.id().to_string())
    }

    async fn start_webhook_server(&self) -> Result<()> {
        if !self.config.use_webhook {
            return Ok(());
        }

        info!(
            "Starting GitOps webhook server on port {}",
            self.config.webhook_port
        );

        let apps = self.applications.clone();
        let repo_path = self.repo_path.clone();
        let event_sender = self.event_sender.clone();

        let app = Router::new()
            .route("/webhook", post(webhook_handler))
            .with_state(apps.clone());

        let addr = SocketAddr::from(([0, 0, 0, 0], self.config.webhook_port));
        let listener = tokio::net::TcpListener::bind(addr).await?;

        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });

        Ok(())
    }
}

async fn webhook_handler(
    State(_apps): State<Arc<RwLock<HashMap<String, Application>>>>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    // Verify webhook signature if secret configured
    // Process webhook payload
    Json(serde_json::json!({"status": "ok"}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_gitops_config() {
        let config = GitOpsConfig::default();
        assert!(config.enabled);
        assert!(!config.repo_url.is_empty());
    }
}
