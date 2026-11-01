//! Scheduler for PN Next

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub max_concurrent_tasks: usize,
    pub default_priority: TaskPriority,
    pub task_timeout: std::time::Duration,
    pub max_retries: u32,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 10,
            default_priority: TaskPriority::Normal,
            task_timeout: std::time::Duration::from_secs(300),
            max_retries: 3,
        }
    }
}

/// Task priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TaskPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Task structure
#[derive(Debug, Clone, Serialize, Deserialize, Eq)]
pub struct Task {
    pub id: Uuid,
    pub name: String,
    pub priority: TaskPriority,
    pub payload: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub timeout: std::time::Duration,
    pub retries: u32,
    pub max_retries: u32,
    pub status: TaskStatus,
    pub assigned_worker: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl Task {
    pub fn new(name: String, payload: serde_json::Value, priority: TaskPriority) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            priority,
            payload,
            created_at: chrono::Utc::now(),
            timeout: std::time::Duration::from_secs(300),
            retries: 0,
            max_retries: 3,
            status: TaskStatus::Pending,
            assigned_worker: None,
        }
    }
}

// Reverse ordering for BinaryHeap (max-heap)
impl Ord for Task {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.cmp(&other.priority).then_with(|| {
            self.created_at
                .timestamp()
                .cmp(&other.created_at.timestamp())
        })
    }
}

impl PartialOrd for Task {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// Scheduler for managing tasks
pub struct Scheduler {
    config: SchedulerConfig,
    task_queue: Arc<tokio::sync::Mutex<BinaryHeap<Task>>>,
    running_tasks: Arc<tokio::sync::RwLock<std::collections::HashMap<uuid::Uuid, Task>>>,
    task_sender: mpsc::Sender<Task>,
    task_receiver: Arc<tokio::sync::Mutex<mpsc::Receiver<Task>>>,
    running: Arc<tokio::sync::Mutex<bool>>,
}

impl Scheduler {
    pub fn new(config: SchedulerConfig) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel(1000);
        Ok(Self {
            config,
            task_queue: Arc::new(tokio::sync::Mutex::new(BinaryHeap::new())),
            running_tasks: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            task_sender: tx,
            task_receiver: Arc::new(tokio::sync::Mutex::new(rx)),
            running: Arc::new(tokio::sync::Mutex::new(false)),
        })
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        *self.running.lock().await = true;

        // Start worker tasks
        for i in 0..self.config.max_concurrent_tasks {
            let receiver = self.task_receiver.clone();
            let running_tasks = self.running_tasks.clone();
            let running = self.running.clone();

            tokio::spawn(async move {
                let mut receiver = receiver.lock().await;
                while *running.lock().await {
                    if let Some(task) = receiver.recv().await {
                        // Execute task
                        // In a real implementation, this would execute the task
                    }
                }
            });
        }

        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        *self.running.lock().await = false;
        Ok(())
    }

    pub async fn submit(&self, task: Task) -> Result<()> {
        self.task_sender.send(task).await?;
        Ok(())
    }

    pub async fn get_task(&self, id: &uuid::Uuid) -> Option<Task> {
        self.running_tasks.read().await.get(id).cloned()
    }

    pub async fn cancel_task(&self, id: &uuid::Uuid) -> Result<()> {
        let mut tasks = self.running_tasks.write().await;
        if let Some(task) = tasks.get_mut(&id) {
            task.status = crate::TaskStatus::Cancelled;
        }
        Ok(())
    }

    pub async fn get_queue_size(&self) -> usize {
        self.task_queue.lock().await.len()
    }

    pub async fn get_running_count(&self) -> usize {
        self.running_tasks.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scheduler_creation() {
        let config = SchedulerConfig::default();
        let scheduler = Scheduler::new(config);
        assert!(scheduler.is_ok());
    }
}
