//! Plugin system for PN Next

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use async_trait::async_trait;

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
    plugins: Arc<tokio::sync::RwLock<HashMap<String, Box<dyn Plugin>>>>,
    hooks: Arc<tokio::sync::RwLock<HashMap<String, Vec<Box<dyn Hook>>>>>,
}

impl PluginManager {
    pub fn new(config: PluginConfig) -> Result<Self> {
        Ok(Self {
            config,
            plugins: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            hooks: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        })
    }
    
    pub async fn load_plugins(&self) -> Result<()> {
        for dir in &self.config.plugin_dirs {
            if dir.exists() {
                let mut entries = tokio::fs::read_dir(dir).await?;
                while let Some(entry) = entries.next_entry().await? {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "so" || ext == "dll" || ext == "dylib") {
                        // In a real implementation, this would load the dynamic library
                        // For now, we'll skip actual plugin loading
                    }
                }
            }
        }
        Ok(())
    }
    
    pub async fn unload_all(&self) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        plugins.clear();
        Ok(())
    }
    
    pub async fn register_plugin(&self, plugin: Box<dyn Plugin>) -> Result<()> {
        let name = plugin.name().to_string();
        let mut plugins = self.plugins.write().await;
        plugins.insert(name, plugin);
        Ok(())
    }
    
    pub async fn unregister_plugin(&self, name: &str) -> Result<()> {
        let mut plugins = self.plugins.write().await;
        plugins.remove(name);
        Ok(())
    }
    
    pub async fn get_plugin(&self, name: &str) -> Option<Box<dyn Plugin>> {
        // Note: Can't return Box<dyn Plugin> from RwLock read guard easily
        // In a real implementation, would use a different pattern
        None
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
        plugins.values().map(|p| p.info()).collect()
    }
}

/// Plugin trait
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

/// Plugin context
pub struct PluginContext {
    pub config: serde_json::Value,
    pub logger: Box<dyn PluginLogger>,
    pub api: Box<dyn PluginApi>,
}

/// Plugin logger
#[async_trait::async_trait]
pub trait PluginLogger: Send + Sync {
    async fn debug(&self, message: &str);
    async fn info(&self, message: &str);
    async fn warn(&self, message: &str);
    async fn error(&self, message: &str);
}

/// Plugin API
#[async_trait::async_trait]
pub trait PluginApi: Send + Sync {
    async fn create_container(&self, config: serde_json::Value) -> Result<String>;
    async fn start_container(&self, id: &str) -> Result<()>;
    async fn stop_container(&self, id: &str) -> Result<()>;
    async fn get_container(&self, id: &str) -> Result<serde_json::Value>;
    async fn list_containers(&self) -> Result<Vec<serde_json::Value>>;
    async fn execute(&self, container: &str, command: Vec<String>) -> Result<String>;
}

/// Plugin event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginEvent {
    ContainerCreated { id: String },
    ContainerStarted { id: String },
    ContainerStopped { id: String },
    ContainerRemoved { id: String },
    ImagePulled { name: String },
    ImageRemoved { name: String },
    NetworkCreated { id: String },
    NetworkRemoved { id: String },
    VolumeCreated { id: String },
    VolumeRemoved { id: String },
    Custom { event_type: String, data: serde_json::Value },
}

/// Hook trait
#[async_trait::async_trait]
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, context: &mut HookContext) -> Result<()>;
    fn priority(&self) -> i32 { 0 }
}

/// Hook context
#[derive(Debug, Clone)]
pub struct HookContext {
    pub event: PluginEvent,
    pub data: serde_json::Value,
    pub metadata: std::collections::HashMap<String, String>,
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