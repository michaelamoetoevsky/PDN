//! Plugin system for PN Next
//!
//! Provides dynamic plugin loading via shared libraries (.so/.dylib/.dll)
//! with a hook system for extensibility.

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use libloading::{Library, Symbol};
use notify::{Event, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub plugin_dirs: Vec<PathBuf>,
    pub auto_load: bool,
    pub allowed_plugins: Vec<String>,
    pub blocked_plugins: Vec<String>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        let mut dirs = Vec::new();
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".pn-next").join("plugins"));
        }
        dirs.push("/usr/local/lib/pn-next/plugins".into());
        dirs.push("/usr/lib/pn-next/plugins".into());

        Self {
            plugin_dirs: dirs,
            auto_load: true,
            allowed_plugins: Vec::new(),
            blocked_plugins: Vec::new(),
        }
    }
}

/// Plugin manager
pub struct PluginManager {
    config: PluginConfig,
    plugins: Arc<RwLock<HashMap<String, Arc<tokio::sync::Mutex<Box<dyn Plugin>>>>>>,
    hooks: Arc<RwLock<HashMap<String, Vec<Box<dyn Hook>>>>>,
    loaded_libraries: Arc<RwLock<HashMap<String, Arc<Library>>>>,
    file_watcher: Option<notify::RecommendedWatcher>,
    watcher_tx: Option<mpsc::Sender<notify::Result<Event>>>,
}

impl PluginManager {
    pub fn new(config: PluginConfig) -> Result<Self> {
        let (tx, rx) = mpsc::channel();

        let mut manager = Self {
            config,
            plugins: Arc::new(RwLock::new(HashMap::new())),
            hooks: Arc::new(RwLock::new(HashMap::new())),
            loaded_libraries: Arc::new(RwLock::new(HashMap::new())),
            file_watcher: None,
            watcher_tx: Some(tx),
        };

        // Start file watcher for auto-reload
        if manager.config.auto_load {
            manager.start_file_watcher(rx)?;
        }

        Ok(manager)
    }

    fn start_file_watcher(&mut self, rx: mpsc::Receiver<notify::Result<Event>>) -> Result<()> {
        let tx = self.watcher_tx.clone().unwrap();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;

        for dir in &self.config.plugin_dirs {
            if dir.exists() {
                watcher.watch(dir, RecursiveMode::NonRecursive)?;
            }
        }

        self.file_watcher = Some(watcher);

        // Spawn watcher handler
        let plugins = self.plugins.clone();
        let loaded_libraries = self.loaded_libraries.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            while let Ok(event) = rx.recv() {
                if let Ok(event) = event {
                    Self::handle_file_event(event, &config, &plugins, &loaded_libraries).await;
                }
            }
        });

        Ok(())
    }

    async fn handle_file_event(
        event: Event,
        config: &PluginConfig,
        plugins: &Arc<RwLock<HashMap<String, Arc<tokio::sync::Mutex<Box<dyn Plugin>>>>>>,
        loaded_libraries: &Arc<RwLock<HashMap<String, Arc<Library>>>>,
    ) {
        for path in event.paths {
            if path.extension().map_or(false, |ext| {
                matches!(ext.to_str(), Some("so" | "dylib" | "dll"))
            }) {
                if event.kind.is_create() || event.kind.is_modify() {
                    // Reload plugin
                    let _ = Self::load_plugin_file(&path, config, plugins, loaded_libraries).await;
                } else if event.kind.is_remove() {
                    // Unload plugin
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        let mut plugins_guard = plugins.write().await;
                        plugins_guard.remove(name);
                        let mut libs_guard = loaded_libraries.write().await;
                        libs_guard.remove(name);
                    }
                }
            }
        }
    }

    async fn load_plugin_file(
        path: &Path,
        config: &PluginConfig,
        plugins: &Arc<RwLock<HashMap<String, Arc<tokio::sync::Mutex<Box<dyn Plugin>>>>>>,
        loaded_libraries: &Arc<RwLock<HashMap<String, Arc<Library>>>>,
    ) -> Result<()> {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid plugin filename"))?;

        // Check allow/block lists
        if !config.allowed_plugins.is_empty() && !config.allowed_plugins.contains(&name.to_string())
        {
            return Ok(());
        }
        if config.blocked_plugins.contains(&name.to_string()) {
            return Ok(());
        }

        // Load the library
        let library = unsafe { Library::new(path) }
            .with_context(|| format!("Failed to load plugin library: {}", path.display()))?;

        let library = Arc::new(library);

        // Get the plugin entry point
        let create_plugin: Symbol<unsafe extern "C" fn() -> *mut dyn Plugin> =
            unsafe { library.get(b"pn_next_create_plugin") }
                .with_context(|| format!("Plugin missing entry point: {}", name))?;

        let plugin_ptr = unsafe { create_plugin() };
        let plugin = unsafe { Box::from_raw(plugin_ptr) };

        // Initialize plugin
        let mut context = PluginContext::default();
        let plugin_arc = Arc::new(tokio::sync::Mutex::new(plugin));
        plugin_arc.lock().await.initialize(&mut context).await?;

        // Store plugin and library
        {
            let mut plugins_guard = plugins.write().await;
            plugins_guard.insert(name.to_string(), plugin_arc);
        }
        {
            let mut libs_guard = loaded_libraries.write().await;
            libs_guard.insert(name.to_string(), library);
        }

        Ok(())
    }

    pub async fn load_plugins(&self) -> Result<()> {
        for dir in &self.config.plugin_dirs {
            if dir.exists() {
                let mut entries = tokio::fs::read_dir(dir).await?;
                while let Some(entry) = entries.next_entry().await? {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| {
                        matches!(ext.to_str(), Some("so" | "dylib" | "dll"))
                    }) {
                        Self::load_plugin_file(
                            &path,
                            &self.config,
                            &self.plugins,
                            &self.loaded_libraries,
                        )
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn unload_all(&self) -> Result<()> {
        // Shutdown all plugins
        let mut plugins = self.plugins.write().await;
        for (_, plugin) in plugins.drain() {
            plugin.lock().await.shutdown().await?;
        }

        // Unload libraries
        let mut libs = self.loaded_libraries.write().await;
        libs.clear();

        Ok(())
    }

    pub async fn register_plugin(&self, plugin: Box<dyn Plugin>) -> Result<()> {
        let name = plugin.name().to_string();
        let plugin_arc = Arc::new(tokio::sync::Mutex::new(plugin));
        let mut plugins = self.plugins.write().await;
        plugins.insert(name, plugin_arc);
        Ok(())
    }

    pub async fn unregister_plugin(&self, name: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        if let Some(plugin) = plugins.remove(name) {
            plugin.lock().await.shutdown().await?;
        }
        let mut libs = self.loaded_libraries.write().await;
        libs.remove(name);
        Ok(())
    }

    pub async fn get_plugin_info(&self, name: &str) -> Option<PluginInfo> {
        let plugins = self.plugins.read().await;
        if let Some(p) = plugins.get(name) {
            let plugin = p.lock().await;
            Some(plugin.info())
        } else {
            None
        }
    }

    pub async fn register_hook(&self, hook_name: &str, hook: Box<dyn Hook>) -> Result<()> {
        let mut hooks = self.hooks.write().await;
        hooks.entry(hook_name.to_string()).or_default().push(hook);
        Ok(())
    }

    pub async fn execute_hook(&self, hook_name: &str, context: &mut HookContext) -> Result<()> {
        let hooks = self.hooks.read().await;
        if let Some(hooks) = hooks.get(hook_name) {
            for hook in hooks {
                hook.execute(context).await?;
            }
        }
        Ok(())
    }

    pub async fn list_plugins(&self) -> Vec<PluginInfo> {
        let plugins = self.plugins.read().await;
        let mut result = Vec::new();
        for p in plugins.values() {
            result.push(p.lock().await.info());
        }
        result
    }
}

/// Plugin trait - implemented by plugin authors
#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn description(&self) -> &str;
    fn author(&self) -> &str;
    fn license(&self) -> &str;
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: self.name().to_string(),
            version: self.version().to_string(),
            description: self.description().to_string(),
            author: self.author().to_string(),
            license: self.license().to_string(),
        }
    }

    async fn initialize(&mut self, context: &mut PluginContext) -> Result<()>;
    async fn shutdown(&mut self) -> Result<()>;
    async fn handle_event(&mut self, event: &PluginEvent) -> Result<()>;
}

/// Plugin information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
}

/// Plugin context provided to plugins during initialization
pub struct PluginContext {
    pub config: serde_json::Value,
    pub logger: Box<dyn PluginLogger>,
    pub api: Box<dyn PluginApi>,
}

impl Default for PluginContext {
    fn default() -> Self {
        Self {
            config: serde_json::Value::Null,
            logger: Box::new(DefaultPluginLogger),
            api: Box::new(DefaultPluginApi),
        }
    }
}

/// Plugin logger trait
#[async_trait::async_trait]
pub trait PluginLogger: Send + Sync {
    async fn debug(&self, message: &str);
    async fn info(&self, message: &str);
    async fn warn(&self, message: &str);
    async fn error(&self, message: &str);
}

/// Default plugin logger
struct DefaultPluginLogger;

#[async_trait::async_trait]
impl PluginLogger for DefaultPluginLogger {
    async fn debug(&self, message: &str) {
        tracing::debug!("[plugin] {}", message);
    }
    async fn info(&self, message: &str) {
        tracing::info!("[plugin] {}", message);
    }
    async fn warn(&self, message: &str) {
        tracing::warn!("[plugin] {}", message);
    }
    async fn error(&self, message: &str) {
        tracing::error!("[plugin] {}", message);
    }
}

/// Plugin API trait - provides access to PN Next functionality
#[async_trait::async_trait]
pub trait PluginApi: Send + Sync {
    async fn create_container(&self, config: serde_json::Value) -> Result<String>;
    async fn start_container(&self, id: &str) -> Result<()>;
    async fn stop_container(&self, id: &str) -> Result<()>;
    async fn get_container(&self, id: &str) -> Result<serde_json::Value>;
    async fn list_containers(&self) -> Result<Vec<serde_json::Value>>;
    async fn execute(&self, container: &str, command: Vec<String>) -> Result<String>;
}

/// Default plugin API (no-op)
struct DefaultPluginApi;

#[async_trait::async_trait]
impl PluginApi for DefaultPluginApi {
    async fn create_container(&self, _config: serde_json::Value) -> Result<String> {
        bail!("Plugin API not available");
    }
    async fn start_container(&self, _id: &str) -> Result<()> {
        bail!("Plugin API not available");
    }
    async fn stop_container(&self, _id: &str) -> Result<()> {
        bail!("Plugin API not available");
    }
    async fn get_container(&self, _id: &str) -> Result<serde_json::Value> {
        bail!("Plugin API not available");
    }
    async fn list_containers(&self) -> Result<Vec<serde_json::Value>> {
        bail!("Plugin API not available");
    }
    async fn execute(&self, _container: &str, _command: Vec<String>) -> Result<String> {
        bail!("Plugin API not available");
    }
}

/// Plugin event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginEvent {
    ContainerCreated {
        id: String,
    },
    ContainerStarted {
        id: String,
    },
    ContainerStopped {
        id: String,
    },
    ContainerRemoved {
        id: String,
    },
    ImagePulled {
        name: String,
    },
    ImageRemoved {
        name: String,
    },
    NetworkCreated {
        id: String,
    },
    NetworkRemoved {
        id: String,
    },
    VolumeCreated {
        id: String,
    },
    VolumeRemoved {
        id: String,
    },
    Custom {
        event_type: String,
        data: serde_json::Value,
    },
}

/// Hook trait
#[async_trait::async_trait]
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, context: &mut HookContext) -> Result<()>;
    fn priority(&self) -> i32 {
        0
    }
}

/// Hook context
#[derive(Debug, Clone)]
pub struct HookContext {
    pub event: PluginEvent,
    pub data: serde_json::Value,
    pub metadata: HashMap<String, String>,
}

/// Built-in plugin example
pub struct ExamplePlugin;

#[async_trait::async_trait]
impl Plugin for ExamplePlugin {
    fn name(&self) -> &str {
        "example"
    }
    fn version(&self) -> &str {
        "0.1.0"
    }
    fn description(&self) -> &str {
        "Example plugin"
    }
    fn author(&self) -> &str {
        "PN Next Team"
    }
    fn license(&self) -> &str {
        "GPL-2.0-or-later"
    }

    async fn initialize(&mut self, _context: &mut PluginContext) -> Result<()> {
        tracing::info!("Example plugin initialized");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<()> {
        tracing::info!("Example plugin shutdown");
        Ok(())
    }

    async fn handle_event(&mut self, _event: &PluginEvent) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_manager() {
        let config = PluginConfig::default();
        let manager = PluginManager::new(config);
        assert!(manager.is_ok());
    }
}
