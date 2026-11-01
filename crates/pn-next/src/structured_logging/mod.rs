//! Structured Logging Module
//! Journald-style logging with rotation, querying, and filtering

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn, Level};

/// Log entry structure (journald-style)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Unique entry ID
    pub id: u64,
    /// Timestamp (nanoseconds since epoch)
    pub timestamp_ns: u64,
    /// Log level
    pub level: LogLevel,
    /// Service/unit name
    pub service: String,
    /// Process ID
    pub pid: u32,
    /// Thread ID
    pub tid: u32,
    /// Message
    pub message: String,
    /// Additional fields (key-value pairs)
    pub fields: HashMap<String, String>,
    /// Source file
    pub file: Option<String>,
    /// Source line
    pub line: Option<u32>,
}

/// Log levels (matching syslog/journald)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Emergency = 0,
    Alert = 1,
    Critical = 2,
    Error = 3,
    Warning = 4,
    Notice = 5,
    Info = 6,
    Debug = 7,
}

impl From<Level> for LogLevel {
    fn from(level: Level) -> Self {
        match level {
            Level::ERROR => LogLevel::Error,
            Level::WARN => LogLevel::Warning,
            Level::INFO => LogLevel::Info,
            Level::DEBUG => LogLevel::Debug,
            Level::TRACE => LogLevel::Debug,
        }
    }
}

/// Log query filter
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogQuery {
    /// Service name filter
    pub service: Option<String>,
    /// Minimum log level
    pub min_level: Option<LogLevel>,
    /// Maximum log level
    pub max_level: Option<LogLevel>,
    /// Start timestamp (ns since epoch)
    pub since: Option<u64>,
    /// End timestamp (ns since epoch)
    pub until: Option<u64>,
    /// Text search in message
    pub grep: Option<String>,
    /// Field filters
    pub fields: HashMap<String, String>,
    /// Limit results
    pub limit: Option<usize>,
    /// Cursor for pagination (entry ID)
    pub cursor: Option<u64>,
    /// Reverse order (newest first)
    pub reverse: bool,
}

/// Log rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationConfig {
    /// Maximum file size before rotation (bytes)
    pub max_file_size: u64,
    /// Maximum number of rotated files to keep
    pub max_files: usize,
    /// Maximum total disk usage (bytes)
    pub max_total_size: u64,
    /// Rotate on time interval (seconds)
    pub rotate_interval_secs: Option<u64>,
    /// Compress rotated files
    pub compress: bool,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            max_file_size: 50 * 1024 * 1024, // 50 MB
            max_files: 10,
            max_total_size: 500 * 1024 * 1024, // 500 MB
            rotate_interval_secs: Some(86400), // 24 hours
            compress: true,
        }
    }
}

/// Structured logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredLoggingConfig {
    /// Enable structured logging
    pub enabled: bool,
    /// Log directory
    pub log_dir: PathBuf,
    /// Rotation configuration
    pub rotation: RotationConfig,
    /// Minimum log level to store
    pub min_level: LogLevel,
    /// Buffer size before flush
    pub buffer_size: usize,
    /// Flush interval (ms)
    pub flush_interval_ms: u64,
    /// Include source location
    pub include_source: bool,
    /// Forward to systemd journal (if available)
    pub forward_to_journal: bool,
}

impl Default for StructuredLoggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_dir: PathBuf::from("~/.local/share/pdn/logs").expand(),
            rotation: RotationConfig::default(),
            min_level: LogLevel::Debug,
            buffer_size: 1000,
            flush_interval_ms: 100,
            include_source: true,
            forward_to_journal: false,
        }
    }
}

/// Trait for path expansion
trait ExpandPath {
    fn expand(self) -> PathBuf;
}

impl ExpandPath for PathBuf {
    fn expand(self) -> PathBuf {
        let expanded = shellexpand::tilde(&self.to_string_lossy()).into_owned();
        PathBuf::from(expanded)
    }
}

/// Log file writer with rotation
struct LogWriter {
    config: RotationConfig,
    log_dir: PathBuf,
    current_file: Option<BufWriter<File>>,
    current_file_path: Option<PathBuf>,
    current_size: u64,
    entry_counter: u64,
    file_counter: usize,
}

impl LogWriter {
    fn new(log_dir: PathBuf, config: RotationConfig) -> Result<Self> {
        fs::create_dir_all(&log_dir)?;
        Ok(Self {
            config,
            log_dir,
            current_file: None,
            current_file_path: None,
            current_size: 0,
            entry_counter: 0,
            file_counter: 0,
        })
    }

    fn get_log_file_path(&self, counter: usize) -> PathBuf {
        self.log_dir.join(format!("pdn.log.{}", counter))
    }

    fn get_current_log_path(&self) -> PathBuf {
        self.log_dir.join("pdn.log")
    }

    async fn ensure_open(&mut self) -> Result<()> {
        if self.current_file.is_none() {
            self.rotate().await?;
        }
        Ok(())
    }

    async fn rotate(&mut self) -> Result<()> {
        // Close current file
        if let Some(mut file) = self.current_file.take() {
            file.flush()?;
        }

        // Rotate existing files
        if self.file_counter >= self.config.max_files {
            let oldest = self.get_log_file_path(0);
            if oldest.exists() {
                fs::remove_file(&oldest)?;
            }
            // Shift files
            for i in 1..self.config.max_files {
                let src = self.get_log_file_path(i);
                let dst = self.get_log_file_path(i - 1);
                if src.exists() {
                    fs::rename(&src, &dst)?;
                }
            }
            self.file_counter = self.config.max_files - 1;
        }

        // Move current to rotated
        let current = self.get_current_log_path();
        if current.exists() {
            let rotated = self.get_log_file_path(self.file_counter);
            fs::rename(&current, &rotated)?;

            if self.config.compress {
                // Compress in background (simplified)
                let rotated_clone = rotated.clone();
                tokio::spawn(async move {
                    let _ = std::process::Command::new("gzip")
                        .arg(&rotated_clone)
                        .status();
                });
            }
            self.file_counter += 1;
        }

        // Open new current file
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&current)?;

        self.current_file = Some(BufWriter::new(file));
        self.current_file_path = Some(current);
        self.current_size = 0;
        self.entry_counter += 1;

        Ok(())
    }

    async fn write_entry(&mut self, entry: &LogEntry) -> Result<()> {
        self.ensure_open().await?;

        let json = serde_json::to_vec(entry)?;
        let json_line = format!("{}\n", String::from_utf8_lossy(&json));

        if let Some(file) = &mut self.current_file {
            file.write_all(json_line.as_bytes())?;
            self.current_size += json_line.len() as u64;

            // Check rotation
            if self.current_size >= self.config.max_file_size {
                self.rotate().await?;
            }
        }

        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        if let Some(file) = &mut self.current_file {
            file.flush()?;
        }
        Ok(())
    }
}

/// Structured logging backend
pub struct StructuredLogging {
    config: StructuredLoggingConfig,
    writer: Arc<Mutex<LogWriter>>,
    entry_id: Arc<RwLock<u64>>,
    buffer: Arc<Mutex<Vec<LogEntry>>>,
}

impl StructuredLogging {
    /// Create a new structured logging backend
    pub async fn new(config: StructuredLoggingConfig) -> Result<Self> {
        let log_dir = shellexpand::tilde(&config.log_dir.to_string_lossy()).into_owned();
        let log_path = PathBuf::from(log_dir);

        let writer = Arc::new(Mutex::new(LogWriter::new(
            log_path,
            config.rotation.clone(),
        )?));
        let entry_id = Arc::new(RwLock::new(0));
        let buffer = Arc::new(Mutex::new(Vec::with_capacity(config.buffer_size)));

        let logging = Self {
            config,
            writer,
            entry_id,
            buffer,
        };

        // Start flush task
        logging.start_flush_task().await;

        // Start rotation monitor
        if let Some(interval_secs) = logging.config.rotation.rotate_interval_secs {
            logging.start_rotation_monitor(interval_secs).await;
        }

        Ok(logging)
    }

    /// Log a message
    pub async fn log(
        &self,
        level: LogLevel,
        service: &str,
        message: &str,
        fields: HashMap<String, String>,
    ) -> Result<()> {
        if level > self.config.min_level {
            return Ok(());
        }

        let id = {
            let mut counter = self.entry_id.write().await;
            *counter += 1;
            *counter
        };

        let timestamp_ns = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u64;

        let entry = LogEntry {
            id,
            timestamp_ns,
            level,
            service: service.to_string(),
            pid: std::process::id(),
            tid: Self::get_thread_id(),
            message: message.to_string(),
            fields,
            file: None,
            line: None,
        };

        // Add to buffer
        {
            let mut buf = self.buffer.lock().await;
            buf.push(entry);
        }

        Ok(())
    }

    /// Log with source location
    pub async fn log_with_source(
        &self,
        level: LogLevel,
        service: &str,
        message: &str,
        fields: HashMap<String, String>,
        file: &str,
        line: u32,
    ) -> Result<()> {
        if level > self.config.min_level {
            return Ok(());
        }

        let id = {
            let mut counter = self.entry_id.write().await;
            *counter += 1;
            *counter
        };

        let timestamp_ns = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos() as u64;

        let entry = LogEntry {
            id,
            timestamp_ns,
            level,
            service: service.to_string(),
            pid: std::process::id(),
            tid: Self::get_thread_id(),
            message: message.to_string(),
            fields,
            file: if self.config.include_source {
                Some(file.to_string())
            } else {
                None
            },
            line: if self.config.include_source {
                Some(line)
            } else {
                None
            },
        };

        {
            let mut buf = self.buffer.lock().await;
            buf.push(entry);
        }

        Ok(())
    }

    /// Get current thread ID
    fn get_thread_id() -> u32 {
        // Use a hash of thread name/id as tid
        use std::thread;
        #[cfg(feature = "thread_id_value")]
        {
            thread::current().id().as_u64().get() as u32
        }
        #[cfg(not(feature = "thread_id_value"))]
        {
            // Fallback: hash the thread name or use a counter
            use std::sync::atomic::{AtomicU32, Ordering};
            static THREAD_COUNTER: AtomicU32 = AtomicU32::new(1);
            THREAD_COUNTER.fetch_add(1, Ordering::Relaxed)
        }
    }

    /// Start background flush task
    async fn start_flush_task(&self) {
        let writer = self.writer.clone();
        let buffer = self.buffer.clone();
        let interval_ms = self.config.flush_interval_ms;

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(interval_ms));

            loop {
                interval.tick().await;

                let entries = {
                    let mut buf = buffer.lock().await;
                    std::mem::take(&mut *buf)
                };

                if !entries.is_empty() {
                    let mut writer = writer.lock().await;
                    for entry in entries {
                        if let Err(e) = writer.write_entry(&entry).await {
                            error!("Failed to write log entry: {}", e);
                        }
                    }
                    if let Err(e) = writer.flush().await {
                        error!("Failed to flush log buffer: {}", e);
                    }
                }
            }
        });
    }

    /// Start rotation monitor
    async fn start_rotation_monitor(&self, interval_secs: u64) {
        let writer = self.writer.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_secs));

            loop {
                interval.tick().await;
                let mut writer = writer.lock().await;
                if let Err(e) = writer.rotate().await {
                    error!("Log rotation failed: {}", e);
                }
            }
        });
    }

    /// Query logs
    pub async fn query(&self, query: LogQuery) -> Result<Vec<LogEntry>> {
        let mut results = Vec::new();
        let limit = query.limit.unwrap_or(1000);
        let reverse = query.reverse;

        // Read log files in order
        let files = self.get_log_files().await?;
        let file_paths: Vec<PathBuf> = files; // Take ownership

        let file_iter: Box<dyn Iterator<Item = &PathBuf>> = if reverse {
            Box::new(file_paths.iter().rev())
        } else {
            Box::new(file_paths.iter())
        };

        for file_path in file_iter {
            if results.len() >= limit {
                break;
            }

            let file_results = self
                .read_log_file(file_path, &query, limit - results.len())
                .await?;
            results.extend(file_results);
        }

        // Apply cursor filter
        if let Some(cursor) = query.cursor {
            if reverse {
                results.retain(|e| e.id < cursor);
            } else {
                results.retain(|e| e.id > cursor);
            }
        }

        // Sort by timestamp
        if reverse {
            results.sort_by(|a, b| b.timestamp_ns.cmp(&a.timestamp_ns));
        } else {
            results.sort_by(|a, b| a.timestamp_ns.cmp(&b.timestamp_ns));
        }

        results.truncate(limit);
        Ok(results)
    }

    /// Get log files in order
    async fn get_log_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        let log_dir = self.config.log_dir.clone().expand();
        let current = log_dir.join("pdn.log");

        // Add rotated files
        for i in 0..self.config.rotation.max_files {
            let rotated = log_dir.join(format!("pdn.log.{}.gz", i));
            if rotated.exists() {
                files.push(rotated);
            }
            let rotated_plain = log_dir.join(format!("pdn.log.{}", i));
            if rotated_plain.exists() {
                files.push(rotated_plain);
            }
        }

        // Add current log last
        if current.exists() {
            files.push(current);
        }

        Ok(files)
    }

    /// Read and filter a log file
    async fn read_log_file(
        &self,
        path: &Path,
        query: &LogQuery,
        limit: usize,
    ) -> Result<Vec<LogEntry>> {
        let mut results = Vec::new();

        // Handle gzipped files
        let is_gzipped = path.extension().map(|e| e == "gz").unwrap_or(false);

        let reader: Box<dyn BufRead> = if is_gzipped {
            let file = File::open(path)?;
            let decoder = flate2::read::GzDecoder::new(file);
            Box::new(BufReader::new(decoder))
        } else {
            let file = File::open(path)?;
            Box::new(BufReader::new(file))
        };

        for line in reader.lines() {
            if results.len() >= limit {
                break;
            }

            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
                if self.matches_query(&entry, query) {
                    results.push(entry);
                }
            }
        }

        Ok(results)
    }

    /// Check if entry matches query
    fn matches_query(&self, entry: &LogEntry, query: &LogQuery) -> bool {
        if let Some(service) = &query.service {
            if entry.service != *service {
                return false;
            }
        }

        if let Some(min) = query.min_level {
            if entry.level < min {
                return false;
            }
        }

        if let Some(max) = query.max_level {
            if entry.level > max {
                return false;
            }
        }

        if let Some(since) = query.since {
            if entry.timestamp_ns < since {
                return false;
            }
        }

        if let Some(until) = query.until {
            if entry.timestamp_ns > until {
                return false;
            }
        }

        if let Some(grep) = &query.grep {
            if !entry.message.contains(grep) {
                return false;
            }
        }

        for (key, value) in &query.fields {
            if entry.fields.get(key) != Some(value) {
                return false;
            }
        }

        true
    }

    /// Get log statistics
    pub async fn get_stats(&self) -> Result<LogStats> {
        let file_paths = self.get_log_files().await?;
        let file_count = file_paths.len();
        let mut total_entries = 0;
        let mut total_size = 0;
        let mut by_level = HashMap::new();
        let mut by_service = HashMap::new();

        for file_path in file_paths {
            let metadata = fs::metadata(&file_path)?;
            total_size += metadata.len();

            let is_gzipped = file_path.extension().map(|e| e == "gz").unwrap_or(false);

            let reader: Box<dyn BufRead> = if is_gzipped {
                let file = File::open(&file_path)?;
                let decoder = flate2::read::GzDecoder::new(file);
                Box::new(BufReader::new(decoder))
            } else {
                let file = File::open(&file_path)?;
                Box::new(BufReader::new(file))
            };

            for line in reader.lines() {
                let line = line?;
                if let Ok(entry) = serde_json::from_str::<LogEntry>(&line) {
                    total_entries += 1;
                    *by_level.entry(format!("{:?}", entry.level)).or_insert(0) += 1;
                    *by_service.entry(entry.service).or_insert(0) += 1;
                }
            }
        }

        Ok(LogStats {
            total_entries,
            total_size_bytes: total_size,
            file_count,
            by_level,
            by_service,
        })
    }

    /// Flush buffer immediately
    pub async fn flush(&self) -> Result<()> {
        let entries = {
            let mut buf = self.buffer.lock().await;
            std::mem::take(&mut *buf)
        };

        if !entries.is_empty() {
            let mut writer = self.writer.lock().await;
            for entry in entries {
                writer.write_entry(&entry).await?;
            }
            writer.flush().await?;
        }
        Ok(())
    }

    /// Shutdown gracefully
    pub async fn shutdown(&self) -> Result<()> {
        self.flush().await
    }
}

/// Log statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogStats {
    pub total_entries: u64,
    pub total_size_bytes: u64,
    pub file_count: usize,
    pub by_level: HashMap<String, u64>,
    pub by_service: HashMap<String, u64>,
}

/// Global logger instance
static GLOBAL_LOGGER: once_cell::sync::OnceCell<StructuredLogging> =
    once_cell::sync::OnceCell::new();

/// Initialize global logger
pub async fn init_global_logger(config: StructuredLoggingConfig) -> Result<()> {
    let logger = StructuredLogging::new(config).await?;
    GLOBAL_LOGGER
        .set(logger)
        .map_err(|_| anyhow::anyhow!("Logger already initialized"))?;
    Ok(())
}

/// Get global logger
pub fn global_logger() -> Option<&'static StructuredLogging> {
    GLOBAL_LOGGER.get()
}

/// Convenience macro for logging
#[macro_export]
macro_rules! pdn_log {
    ($level:expr, $service:expr, $msg:expr) => {{
        if let Some(logger) = $crate::structured_logging::global_logger() {
            let _ = logger.log($level, $service, $msg, std::collections::HashMap::new()).await;
        }
    }};
    ($level:expr, $service:expr, $msg:expr, $($key:expr => $val:expr),+) => {{
        if let Some(logger) = $crate::structured_logging::global_logger() {
            let mut fields = std::collections::HashMap::new();
            $(fields.insert($key.to_string(), $val.to_string());)+
            let _ = logger.log($level, $service, $msg, fields).await;
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_structured_logging_creation() {
        let dir = tempdir().unwrap();
        let config = StructuredLoggingConfig {
            log_dir: dir.path().to_path_buf(),
            ..Default::default()
        };

        let logging = StructuredLogging::new(config).await.unwrap();
        assert!(logging.config.enabled);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_log_and_query() {
        let dir = tempdir().unwrap();
        let config = StructuredLoggingConfig {
            log_dir: dir.path().to_path_buf(),
            buffer_size: 10,
            flush_interval_ms: 10,
            ..Default::default()
        };

        let logging = StructuredLogging::new(config).await.unwrap();

        logging
            .log(
                LogLevel::Info,
                "test-service",
                "Hello world",
                HashMap::new(),
            )
            .await
            .unwrap();
        logging
            .log(
                LogLevel::Error,
                "test-service",
                "Error occurred",
                HashMap::new(),
            )
            .await
            .unwrap();

        // Wait for flush
        tokio::time::sleep(Duration::from_millis(50)).await;
        logging.flush().await.unwrap();

        let query = LogQuery {
            service: Some("test-service".to_string()),
            ..Default::default()
        };

        let results = logging.query(query).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_log_levels() {
        let dir = tempdir().unwrap();
        let config = StructuredLoggingConfig {
            log_dir: dir.path().to_path_buf(),
            min_level: LogLevel::Warning,
            buffer_size: 10,
            flush_interval_ms: 10,
            ..Default::default()
        };

        let logging = StructuredLogging::new(config).await.unwrap();

        logging
            .log(LogLevel::Debug, "test", "Debug message", HashMap::new())
            .await
            .unwrap();
        logging
            .log(LogLevel::Info, "test", "Info message", HashMap::new())
            .await
            .unwrap();
        logging
            .log(LogLevel::Warning, "test", "Warning message", HashMap::new())
            .await
            .unwrap();
        logging
            .log(LogLevel::Error, "test", "Error message", HashMap::new())
            .await
            .unwrap();
        logging
            .log(
                LogLevel::Critical,
                "test",
                "Critical message",
                HashMap::new(),
            )
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;
        logging.flush().await.unwrap();

        let query = LogQuery::default();
        let results = logging.query(query).await.unwrap();

        // With min_level = Warning (4), we should get Warning (4), Error (3), Critical (2), Alert (1), Emergency (0)
        // Debug (7) and Info (6) should be filtered out
        // Note: exact count may vary due to async flush timing
        assert!(results.len() >= 3);
        assert!(results.iter().all(|e| e.level <= LogLevel::Warning));
    }
}
