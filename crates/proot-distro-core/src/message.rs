//! Message system for PRoot Distro

use std::fmt;
use serde::{Serialize, Deserialize};

/// Message level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageLevel {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl fmt::Display for MessageLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageLevel::Debug => write!(f, "DEBUG"),
            MessageLevel::Info => write!(f, "INFO"),
            MessageLevel::Warning => write!(f, "WARN"),
            MessageLevel::Error => write!(f, "ERROR"),
            MessageLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub level: MessageLevel,
    pub source: String,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl Message {
    pub fn new(level: MessageLevel, source: String, message: String) -> Self {
        Self {
            level,
            source,
            message,
            timestamp: chrono::Utc::now(),
            metadata: std::collections::HashMap::new(),
        }
    }
    
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Message handler
pub trait MessageHandler: Send + Sync {
    fn handle(&self, message: &Message);
}

/// Default message handler
pub struct DefaultMessageHandler;

impl MessageHandler for DefaultMessageHandler {
    fn handle(&self, message: &Message) {
        let level_str = match message.level {
            MessageLevel::Debug => "\x1b[36mDEBUG\x1b[0m",
            MessageLevel::Info => "\x1b[32mINFO\x1b[0m",
            MessageLevel::Warning => "\x1b[33mWARN\x1b[0m",
            MessageLevel::Error => "\x1b[31mERROR\x1b[0m",
            MessageLevel::Critical => "\x1b[31;1mCRITICAL\x1b[0m",
        };
        
        eprintln!("{} [{}] {}: {}", 
            message.timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
            level_str,
            message.source,
            message.message
        );
    }
}