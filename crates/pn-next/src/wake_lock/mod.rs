//! Wake Lock Management Module
//! Keeps services alive during Android doze mode and battery optimization

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Wake lock types
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub enum WakeLockType {
    /// Partial wake lock - CPU stays on, screen off
    Partial,
    /// Full wake lock - CPU and screen on
    Full,
    /// Doze mode exemption - allow network access in doze
    DozeExemption,
}

impl Default for WakeLockType {
    fn default() -> Self {
        WakeLockType::Partial
    }
}

/// Wake lock configuration
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct WakeLockConfig {
    /// Enable wake lock management
    pub enabled: bool,
    /// Default wake lock type
    pub default_type: WakeLockType,
    /// Services that require wake locks
    pub service_wake_locks: HashMap<String, WakeLockType>,
    /// Acquire timeout (milliseconds)
    pub acquire_timeout_ms: u64,
    /// Release timeout (milliseconds)
    pub release_timeout_ms: u64,
    /// Auto-release after inactivity (seconds)
    pub auto_release_secs: Option<u64>,
    /// Battery optimization whitelist
    pub battery_whitelist: Vec<String>,
}

impl Default for WakeLockConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_type: WakeLockType::Partial,
            service_wake_locks: HashMap::new(),
            acquire_timeout_ms: 5000,
            release_timeout_ms: 1000,
            auto_release_secs: Some(3600), // 1 hour
            battery_whitelist: Vec::new(),
        }
    }
}

/// Wake lock state
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct WakeLockState {
    /// Service name
    pub service_name: String,
    /// Wake lock type
    pub lock_type: WakeLockType,
    /// Whether currently held
    pub held: bool,
    /// Acquired timestamp
    pub acquired_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Reference count (for shared locks)
    pub ref_count: u32,
    /// PID of holder
    pub holder_pid: Option<u32>,
}

/// Wake lock manager
pub struct WakeLockManager {
    config: WakeLockConfig,
    locks: Arc<RwLock<HashMap<String, WakeLockState>>>,
    // Platform-specific implementation
    #[cfg(target_os = "android")]
    power_manager: Option<Arc<Mutex<AndroidPowerManager>>>,
}

#[cfg(target_os = "android")]
struct AndroidPowerManager {
    // JNI handle to Android PowerManager
    // In practice, this would use jni crate to call PowerManager APIs
    // context: jni::JavaVM,
    _placeholder: (),
}

impl WakeLockManager {
    /// Create a new wake lock manager
    pub fn new(config: WakeLockConfig) -> Self {
        Self {
            config,
            locks: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(target_os = "android")]
            power_manager: None,
        }
    }

    /// Initialize Android power manager (Android only)
    #[cfg(target_os = "android")]
    pub async fn initialize_android(&mut self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Initialize JNI and get PowerManager
        // This is a simplified version - real implementation would use jni crate
        info!("Android wake lock manager initialized (stub)");
        Ok(())
    }

    /// Initialize (no-op on non-Android)
    #[cfg(not(target_os = "android"))]
    pub async fn initialize_android(&mut self) -> Result<()> {
        info!("Wake lock manager initialized (non-Android platform)");
        Ok(())
    }

    /// Acquire a wake lock for a service
    pub async fn acquire(&self, service_name: &str, lock_type: Option<WakeLockType>) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let lock_type = lock_type
            .or_else(|| self.config.service_wake_locks.get(service_name).copied())
            .unwrap_or(self.config.default_type);

        let mut locks = self.locks.write().await;

        let mut state = locks
            .entry(service_name.to_string())
            .or_insert_with(|| WakeLockState {
                service_name: service_name.to_string(),
                lock_type,
                held: false,
                acquired_at: None,
                ref_count: 0,
                holder_pid: None,
            });

        if state.held && state.lock_type == lock_type {
            // Already held with same type, increment ref count
            state.ref_count += 1;
            debug!("Wake lock ref++ for {}: {}", service_name, state.ref_count);
            return Ok(());
        }

        if state.held && state.lock_type != lock_type {
            // Different type held, release first - need to drop the lock first
            drop(locks);
            self.release(service_name).await?;
            locks = self.locks.write().await;
            state = locks
                .entry(service_name.to_string())
                .or_insert_with(|| WakeLockState {
                    service_name: service_name.to_string(),
                    lock_type,
                    held: false,
                    acquired_at: None,
                    ref_count: 0,
                    holder_pid: None,
                });
        }

        // Acquire new lock
        self.acquire_platform(service_name, lock_type).await?;

        state.held = true;
        state.lock_type = lock_type;
        state.acquired_at = Some(chrono::Utc::now());
        state.ref_count = 1;
        state.holder_pid = Some(std::process::id());

        info!(
            "Acquired {:?} wake lock for service: {}",
            lock_type, service_name
        );
        Ok(())
    }

    /// Release a wake lock for a service
    pub async fn release(&self, service_name: &str) -> Result<()> {
        let mut locks = self.locks.write().await;
        self.release_internal(service_name, &mut locks).await
    }

    /// Internal release implementation
    async fn release_internal(
        &self,
        service_name: &str,
        locks: &mut HashMap<String, WakeLockState>,
    ) -> Result<()> {
        if let Some(state) = locks.get_mut(service_name) {
            if state.ref_count > 1 {
                state.ref_count -= 1;
                debug!("Wake lock ref-- for {}: {}", service_name, state.ref_count);
                return Ok(());
            }

            if state.held {
                self.release_platform(service_name, state.lock_type).await?;

                state.held = false;
                state.acquired_at = None;
                state.ref_count = 0;
                state.holder_pid = None;

                info!("Released wake lock for service: {}", service_name);
            }
        }
        Ok(())
    }

    /// Platform-specific acquire
    async fn acquire_platform(&self, service_name: &str, lock_type: WakeLockType) -> Result<()> {
        #[cfg(target_os = "android")]
        {
            // Real implementation would call PowerManager.newWakeLock()
            debug!(
                "Android: Would acquire {:?} wake lock for {}",
                lock_type, service_name
            );
        }

        #[cfg(not(target_os = "android"))]
        {
            // On non-Android, simulate with a file lock or no-op
            debug!(
                "Non-Android: Simulated acquire {:?} wake lock for {}",
                lock_type, service_name
            );
        }

        Ok(())
    }

    /// Platform-specific release
    async fn release_platform(&self, service_name: &str, lock_type: WakeLockType) -> Result<()> {
        #[cfg(target_os = "android")]
        {
            // Real implementation would call wakeLock.release()
            debug!(
                "Android: Would release {:?} wake lock for {}",
                lock_type, service_name
            );
        }

        #[cfg(not(target_os = "android"))]
        {
            debug!(
                "Non-Android: Simulated release {:?} wake lock for {}",
                lock_type, service_name
            );
        }

        Ok(())
    }

    /// Get wake lock state for a service
    pub async fn get_state(&self, service_name: &str) -> Option<WakeLockState> {
        let locks = self.locks.read().await;
        locks.get(service_name).cloned()
    }

    /// Get all wake lock states
    pub async fn get_all_states(&self) -> Vec<WakeLockState> {
        let locks = self.locks.read().await;
        locks.values().cloned().collect()
    }

    /// Check if a service has an active wake lock
    pub async fn is_held(&self, service_name: &str) -> bool {
        let locks = self.locks.read().await;
        locks.get(service_name).map(|s| s.held).unwrap_or(false)
    }

    /// Start auto-release monitor
    pub async fn start_auto_release_monitor(&self) -> Result<()> {
        if let Some(auto_release_secs) = self.config.auto_release_secs {
            let locks = self.locks.clone();
            let config = self.config.clone();

            tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(60)); // Check every minute

                loop {
                    interval.tick().await;

                    let mut locks_write = locks.write().await;
                    let now = chrono::Utc::now();

                    for (name, state) in locks_write.iter_mut() {
                        if state.held {
                            if let Some(acquired) = state.acquired_at {
                                let elapsed = now.signed_duration_since(acquired).num_seconds();
                                if elapsed >= auto_release_secs as i64 {
                                    warn!(
                                        "Auto-releasing wake lock for {} after {}s",
                                        name, elapsed
                                    );
                                    state.held = false;
                                    state.acquired_at = None;
                                    state.ref_count = 0;
                                    state.holder_pid = None;
                                }
                            }
                        }
                    }
                }
            });
        }
        Ok(())
    }

    /// Request battery optimization exemption (Android)
    pub async fn request_battery_exemption(&self, service_name: &str) -> Result<bool> {
        if !self
            .config
            .battery_whitelist
            .contains(&service_name.to_string())
        {
            return Ok(false);
        }

        #[cfg(target_os = "android")]
        {
            // Real implementation would use PowerManager.isIgnoringBatteryOptimizations()
            // and request via Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS
            debug!(
                "Android: Would request battery exemption for {}",
                service_name
            );
            return Ok(true);
        }

        #[cfg(not(target_os = "android"))]
        {
            debug!("Non-Android: Battery exemption not applicable");
            return Ok(false);
        }
    }

    /// Add service to battery whitelist
    pub fn add_to_battery_whitelist(&mut self, service_name: &str) {
        if !self
            .config
            .battery_whitelist
            .contains(&service_name.to_string())
        {
            self.config.battery_whitelist.push(service_name.to_string());
        }
    }

    /// Set wake lock type for a service
    pub fn set_service_wake_lock(&mut self, service_name: &str, lock_type: WakeLockType) {
        self.config
            .service_wake_locks
            .insert(service_name.to_string(), lock_type);
    }

    /// Release all wake locks
    pub async fn release_all(&self) -> Result<()> {
        let mut locks = self.locks.write().await;
        let service_names: Vec<String> = locks.keys().cloned().collect();

        for name in service_names {
            self.release_internal(&name, &mut locks).await?;
        }

        info!("Released all wake locks");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_wake_lock_manager_creation() {
        let config = WakeLockConfig::default();
        let manager = WakeLockManager::new(config);
        assert!(manager.config.enabled);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_acquire_release() {
        let config = WakeLockConfig::default();
        let manager = WakeLockManager::new(config);

        manager
            .acquire("test-service", Some(WakeLockType::Partial))
            .await
            .unwrap();
        assert!(manager.is_held("test-service").await);

        manager.release("test-service").await.unwrap();
        assert!(!manager.is_held("test-service").await);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_ref_counting() {
        let config = WakeLockConfig::default();
        let manager = WakeLockManager::new(config);

        manager
            .acquire("test-service", Some(WakeLockType::Partial))
            .await
            .unwrap();
        manager
            .acquire("test-service", Some(WakeLockType::Partial))
            .await
            .unwrap();

        let state = manager.get_state("test-service").await.unwrap();
        assert_eq!(state.ref_count, 2);

        manager.release("test-service").await.unwrap();
        let state = manager.get_state("test-service").await.unwrap();
        assert_eq!(state.ref_count, 1);
        assert!(state.held);

        manager.release("test-service").await.unwrap();
        assert!(!manager.is_held("test-service").await);
    }
}
