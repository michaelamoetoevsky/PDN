//! Multi-node Clustering Module
//! Distributed scheduling and cluster management for PDN

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{broadcast, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Cluster configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// Enable clustering
    pub enabled: bool,
    /// This node's ID
    pub node_id: String,
    /// This node's address
    pub node_addr: SocketAddr,
    /// Cluster name
    pub cluster_name: String,
    /// Seed nodes for joining cluster
    pub seed_nodes: Vec<SocketAddr>,
    /// Heartbeat interval (seconds)
    pub heartbeat_interval_secs: u64,
    /// Node timeout (seconds)
    pub node_timeout_secs: u64,
    /// Enable leader election
    pub leader_election: bool,
    /// Scheduler config
    pub scheduler: SchedulerConfig,
    /// Gossip protocol config
    pub gossip: GossipConfig,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            node_id: Uuid::new_v4().to_string(),
            node_addr: "0.0.0.0:7946".parse().unwrap(),
            cluster_name: "pdn-cluster".to_string(),
            seed_nodes: Vec::new(),
            heartbeat_interval_secs: 5,
            node_timeout_secs: 30,
            leader_election: true,
            scheduler: SchedulerConfig::default(),
            gossip: GossipConfig::default(),
        }
    }
}

/// Scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Enable distributed scheduler
    pub enabled: bool,
    /// Scheduling algorithm
    pub algorithm: SchedulingAlgorithm,
    /// Resource weights for scoring
    pub cpu_weight: f64,
    pub memory_weight: f64,
    pub disk_weight: f64,
    pub network_weight: f64,
    /// Affinity/anti-affinity
    pub enable_affinity: bool,
    pub enable_anti_affinity: bool,
    /// Max containers per node
    pub max_containers_per_node: usize,
    /// Resource overcommit ratio
    pub cpu_overcommit_ratio: f64,
    pub memory_overcommit_ratio: f64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            algorithm: SchedulingAlgorithm::BinPacking,
            cpu_weight: 1.0,
            memory_weight: 1.0,
            disk_weight: 0.5,
            network_weight: 0.5,
            enable_affinity: true,
            enable_anti_affinity: true,
            max_containers_per_node: 100,
            cpu_overcommit_ratio: 2.0,
            memory_overcommit_ratio: 1.5,
        }
    }
}

/// Scheduling algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulingAlgorithm {
    RoundRobin,
    BinPacking,
    Spread,
    LeastLoaded,
    MostLoaded,
    Custom,
}

/// Gossip protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipConfig {
    /// Enable gossip
    pub enabled: bool,
    /// Gossip interval (seconds)
    pub interval_secs: u64,
    /// Fanout (number of nodes to gossip to)
    pub fanout: usize,
    /// Max message size
    pub max_message_size: usize,
    /// Gossip port
    pub port: u16,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_secs: 1,
            fanout: 3,
            max_message_size: 65536,
            port: 7947,
        }
    }
}

/// Node information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub addr: SocketAddr,
    pub status: NodeStatus,
    pub role: NodeRole,
    pub resources: NodeResources,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
    pub started_at: SystemTime,
    pub last_heartbeat: SystemTime,
    pub version: String,
    pub runtime: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Unknown,
    Starting,
    Ready,
    NotReady,
    Unreachable,
    Terminating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    Worker,
    Manager,
    Leader,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResources {
    pub cpu_total: f64,
    pub cpu_allocated: f64,
    pub cpu_available: f64,
    pub memory_total: u64,
    pub memory_allocated: u64,
    pub memory_available: u64,
    pub disk_total: u64,
    pub disk_allocated: u64,
    pub disk_available: u64,
    pub network_rx: u64,
    pub network_tx: u64,
    pub containers_running: usize,
    pub containers_total: usize,
}

/// Cluster state
#[derive(Debug, Clone)]
pub struct ClusterState {
    pub config: ClusterConfig,
    pub nodes: Arc<RwLock<HashMap<String, NodeInfo>>>,
    pub local_node: NodeInfo,
    pub leader: Arc<RwLock<Option<String>>>,
    pub scheduler_state: Arc<RwLock<SchedulerState>>,
    pub event_sender: broadcast::Sender<ClusterEvent>,
    pub start_time: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerState {
    pub pending_pods: Vec<PodSpec>,
    pub scheduled_pods: HashMap<String, String>, // pod_id -> node_id
    pub node_scores: HashMap<String, f64>,
}

impl Default for SchedulerState {
    fn default() -> Self {
        Self {
            pending_pods: Vec::new(),
            scheduled_pods: HashMap::new(),
            node_scores: HashMap::new(),
        }
    }
}

/// Pod specification for scheduling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodSpec {
    pub id: String,
    pub name: String,
    pub namespace: String,
    pub containers: Vec<ContainerSpec>,
    pub resources: ResourceRequirements,
    pub affinity: Option<Affinity>,
    pub tolerations: Vec<Toleration>,
    pub priority: i32,
    pub scheduler_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerSpec {
    pub name: String,
    pub image: String,
    pub command: Vec<String>,
    pub args: Vec<String>,
    pub resources: ResourceRequirements,
    pub ports: Vec<PortSpec>,
    pub env: Vec<EnvVar>,
    pub volume_mounts: Vec<VolumeMount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub cpu: f64,
    pub memory: u64,
    pub disk: u64,
    pub network: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Affinity {
    pub node_affinity: Option<NodeAffinity>,
    pub pod_affinity: Option<PodAffinity>,
    pub pod_anti_affinity: Option<PodAntiAffinity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeAffinity {
    pub required_during_scheduling: Vec<NodeSelectorTerm>,
    pub preferred_during_scheduling: Vec<PreferredSchedulingTerm>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodAffinity {
    pub required_during_scheduling: Vec<PodAffinityTerm>,
    pub preferred_during_scheduling: Vec<WeightedPodAffinityTerm>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodAntiAffinity {
    pub required_during_scheduling: Vec<PodAffinityTerm>,
    pub preferred_during_scheduling: Vec<WeightedPodAffinityTerm>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSelectorTerm {
    pub match_expressions: Vec<NodeSelectorRequirement>,
    pub match_fields: Vec<NodeSelectorRequirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSelectorRequirement {
    pub key: String,
    pub operator: NodeSelectorOperator,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeSelectorOperator {
    In,
    NotIn,
    Exists,
    DoesNotExist,
    Gt,
    Lt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreferredSchedulingTerm {
    pub weight: i32,
    pub preference: NodeSelectorTerm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodAffinityTerm {
    pub label_selector: LabelSelector,
    pub namespaces: Vec<String>,
    pub topology_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedPodAffinityTerm {
    pub weight: i32,
    pub pod_affinity_term: PodAffinityTerm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelSelector {
    pub match_labels: HashMap<String, String>,
    pub match_expressions: Vec<LabelSelectorRequirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelSelectorRequirement {
    pub key: String,
    pub operator: LabelSelectorOperator,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelSelectorOperator {
    In,
    NotIn,
    Exists,
    DoesNotExist,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Toleration {
    pub key: String,
    pub operator: TolerationOperator,
    pub value: String,
    pub effect: TaintEffect,
    pub toleration_seconds: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TolerationOperator {
    Exists,
    Equal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaintEffect {
    NoSchedule,
    PreferNoSchedule,
    NoExecute,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortSpec {
    pub container_port: i32,
    pub host_port: Option<i32>,
    pub protocol: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMount {
    pub name: String,
    pub mount_path: String,
    pub read_only: bool,
    pub sub_path: String,
}

/// Cluster events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClusterEvent {
    NodeJoined {
        node_id: String,
        addr: SocketAddr,
    },
    NodeLeft {
        node_id: String,
    },
    NodeStatusChanged {
        node_id: String,
        old_status: NodeStatus,
        new_status: NodeStatus,
    },
    LeaderElected {
        node_id: String,
    },
    LeaderLost {
        node_id: String,
    },
    PodScheduled {
        pod_id: String,
        node_id: String,
    },
    PodUnscheduled {
        pod_id: String,
    },
    ResourceUpdated {
        node_id: String,
    },
    PartitionDetected {
        nodes: Vec<String>,
    },
}

/// Cluster manager
pub struct ClusterManager {
    state: Arc<ClusterState>,
}

impl ClusterManager {
    pub fn new(config: ClusterConfig) -> Self {
        let (event_sender, _) = broadcast::channel(1000);

        let local_node = NodeInfo {
            id: config.node_id.clone(),
            name: format!("pdn-{}", &config.node_id[..8]),
            addr: config.node_addr,
            status: NodeStatus::Starting,
            role: if config.leader_election {
                NodeRole::Manager
            } else {
                NodeRole::Worker
            },
            resources: NodeResources::default(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
            started_at: SystemTime::now(),
            last_heartbeat: SystemTime::now(),
            version: "0.1.0".to_string(),
            runtime: "pdn".to_string(),
        };

        let mut nodes = HashMap::new();
        nodes.insert(local_node.id.clone(), local_node.clone());

        let state = Arc::new(ClusterState {
            config: config.clone(),
            nodes: Arc::new(RwLock::new(nodes)),
            local_node: local_node.clone(),
            leader: Arc::new(RwLock::new(None)),
            scheduler_state: Arc::new(RwLock::new(SchedulerState::default())),
            event_sender,
            start_time: Instant::now(),
        });

        Self { state }
    }

    pub async fn start(&self) -> Result<()> {
        if !self.state.config.enabled {
            info!("Cluster disabled");
            return Ok(());
        }

        info!("Starting cluster manager: {}", self.state.config.node_id);

        // Update local node status
        self.update_local_status(NodeStatus::Ready).await;

        // Start background tasks
        self.start_heartbeat_task().await;
        self.start_gossip_task().await;

        if self.state.config.leader_election {
            self.start_leader_election_task().await;
        }

        if self.state.config.scheduler.enabled {
            self.start_scheduler_task().await;
        }

        // Join cluster if seed nodes provided
        if !self.state.config.seed_nodes.is_empty() {
            self.join_cluster().await?;
        }

        Ok(())
    }

    async fn update_local_status(&self, status: NodeStatus) {
        let mut nodes = self.state.nodes.write().await;
        if let Some(node) = nodes.get_mut(&self.state.config.node_id) {
            node.status = status;
            node.last_heartbeat = SystemTime::now();
        }
    }

    async fn start_heartbeat_task(&self) {
        let state = self.state.clone();
        let interval_secs = state.config.heartbeat_interval_secs;
        let timeout_secs = state.config.node_timeout_secs;

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_secs));

            loop {
                interval.tick().await;

                // Update local heartbeat
                {
                    let mut nodes = state.nodes.write().await;
                    if let Some(node) = nodes.get_mut(&state.config.node_id) {
                        node.last_heartbeat = SystemTime::now();
                        node.status = NodeStatus::Ready;
                    }
                }

                // Check for failed nodes
                let now = SystemTime::now();
                let mut failed_nodes = Vec::new();

                {
                    let nodes = state.nodes.read().await;
                    for (id, node) in nodes.iter() {
                        if id == &state.config.node_id {
                            continue;
                        }
                        if let Ok(elapsed) = now.duration_since(node.last_heartbeat) {
                            if elapsed > Duration::from_secs(timeout_secs) {
                                failed_nodes.push(id.clone());
                            }
                        }
                    }
                }

                // Mark failed nodes
                for id in failed_nodes {
                    let mut nodes = state.nodes.write().await;
                    if let Some(node) = nodes.get_mut(&id) {
                        node.status = NodeStatus::Unreachable;
                        info!("Node {} marked as unreachable", id);
                    }

                    state
                        .event_sender
                        .send(ClusterEvent::NodeStatusChanged {
                            node_id: id.clone(),
                            old_status: NodeStatus::Ready,
                            new_status: NodeStatus::Unreachable,
                        })
                        .ok();
                }
            }
        });
    }

    async fn start_gossip_task(&self) {
        if !self.state.config.gossip.enabled {
            return;
        }

        let state = self.state.clone();
        let interval_secs = state.config.gossip.interval_secs;

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_secs));

            loop {
                interval.tick().await;

                // Gossip protocol implementation
                // In a real implementation, this would send cluster state to random nodes
                debug!("Gossip tick");
            }
        });
    }

    async fn start_leader_election_task(&self) {
        let state = self.state.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10));

            loop {
                interval.tick().await;

                let current_leader = state.leader.read().await.clone();

                // Simple leader election: node with lowest ID becomes leader
                let nodes = state.nodes.read().await;
                let ready_nodes: Vec<_> = nodes
                    .values()
                    .filter(|n| n.status == NodeStatus::Ready && n.role != NodeRole::Worker)
                    .collect();

                if !ready_nodes.is_empty() {
                    let new_leader = ready_nodes
                        .iter()
                        .min_by_key(|n| n.id.clone())
                        .map(|n| n.id.clone());

                    drop(nodes);

                    let mut leader = state.leader.write().await;
                    if *leader != new_leader {
                        if let Some(old_leader) = current_leader {
                            state
                                .event_sender
                                .send(ClusterEvent::LeaderLost {
                                    node_id: old_leader,
                                })
                                .ok();
                        }
                        if let Some(new_leader_id) = new_leader.clone() {
                            state
                                .event_sender
                                .send(ClusterEvent::LeaderElected {
                                    node_id: new_leader_id,
                                })
                                .ok();
                        }
                        *leader = new_leader;
                    }
                }
            }
        });
    }

    async fn start_scheduler_task(&self) {
        let state = self.state.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));

            loop {
                interval.tick().await;

                // Schedule pending pods
                let mut scheduler_state = state.scheduler_state.write().await;
                let nodes = state.nodes.read().await;

                let mut pending = Vec::new();
                std::mem::swap(&mut scheduler_state.pending_pods, &mut pending);

                for pod in pending {
                    if let Some(node_id) =
                        Self::schedule_pod(&pod, &nodes, &state.config.scheduler).await
                    {
                        scheduler_state
                            .scheduled_pods
                            .insert(pod.id.clone(), node_id.clone());
                        info!("Scheduled pod {} to node {}", pod.id, node_id);
                    } else {
                        // Re-queue
                        scheduler_state.pending_pods.push(pod);
                    }
                }
            }
        });
    }

    async fn schedule_pod(
        pod: &PodSpec,
        nodes: &HashMap<String, NodeInfo>,
        scheduler_config: &SchedulerConfig,
    ) -> Option<String> {
        let mut best_node: Option<String> = None;
        let mut best_score: f64 = f64::MIN;

        for (id, node) in nodes {
            if node.status != NodeStatus::Ready {
                continue;
            }

            if node.resources.containers_running >= scheduler_config.max_containers_per_node {
                continue;
            }

            // Check resource availability
            if node.resources.cpu_available
                < pod.resources.cpu * scheduler_config.cpu_overcommit_ratio
            {
                continue;
            }
            if node.resources.memory_available < pod.resources.memory {
                continue;
            }

            // Check affinity/anti-affinity
            if !Self::check_affinity(pod, node, scheduler_config) {
                continue;
            }

            // Score node based on algorithm
            let score = match scheduler_config.algorithm {
                SchedulingAlgorithm::BinPacking => {
                    // Prefer nodes with less available resources (pack tightly)
                    let cpu_util =
                        1.0 - (node.resources.cpu_available / node.resources.cpu_total.max(1.0));
                    let mem_util = 1.0
                        - (node.resources.memory_available as f64
                            / node.resources.memory_total.max(1) as f64);
                    (cpu_util * scheduler_config.cpu_weight
                        + mem_util * scheduler_config.memory_weight)
                        * 100.0
                }
                SchedulingAlgorithm::Spread => {
                    // Prefer nodes with more available resources (spread out)
                    let cpu_avail =
                        node.resources.cpu_available / node.resources.cpu_total.max(1.0);
                    let mem_avail = node.resources.memory_available as f64
                        / node.resources.memory_total.max(1) as f64;
                    (cpu_avail * scheduler_config.cpu_weight
                        + mem_avail * scheduler_config.memory_weight)
                        * 100.0
                }
                SchedulingAlgorithm::LeastLoaded => {
                    // Prefer node with least containers
                    -(node.resources.containers_running as f64)
                }
                SchedulingAlgorithm::MostLoaded => {
                    // Prefer node with most containers
                    node.resources.containers_running as f64
                }
                SchedulingAlgorithm::RoundRobin => {
                    // Simple hash-based
                    pod.id.len() as f64 % nodes.len() as f64
                }
                SchedulingAlgorithm::Custom => 0.0,
            };

            if score > best_score {
                best_score = score;
                best_node = Some(id.clone());
            }
        }

        best_node
    }

    fn check_affinity(pod: &PodSpec, node: &NodeInfo, config: &SchedulerConfig) -> bool {
        if !config.enable_affinity && !config.enable_anti_affinity {
            return true;
        }

        if let Some(affinity) = &pod.affinity {
            // Check node affinity
            if let Some(node_affinity) = &affinity.node_affinity {
                for term in &node_affinity.required_during_scheduling {
                    for req in &term.match_expressions {
                        let node_value = node.labels.get(&req.key);
                        match (req.operator, node_value) {
                            (NodeSelectorOperator::In, Some(v)) if !req.values.contains(v) => {
                                return false
                            }
                            (NodeSelectorOperator::NotIn, Some(v)) if req.values.contains(v) => {
                                return false
                            }
                            (NodeSelectorOperator::Exists, None) => return false,
                            (NodeSelectorOperator::DoesNotExist, Some(_)) => return false,
                            _ => {}
                        }
                    }
                }
            }
        }

        true
    }

    pub async fn join_cluster(&self) -> Result<()> {
        info!(
            "Joining cluster with seed nodes: {:?}",
            self.state.config.seed_nodes
        );

        for seed_addr in &self.state.config.seed_nodes {
            // In a real implementation, this would connect to seed node
            // and exchange cluster state
            info!("Attempting to join seed node: {}", seed_addr);
        }

        Ok(())
    }

    pub async fn submit_pod(&self, pod: PodSpec) -> Result<()> {
        let mut scheduler_state = self.state.scheduler_state.write().await;
        scheduler_state.pending_pods.push(pod);
        Ok(())
    }

    pub async fn get_cluster_state(&self) -> ClusterStateInfo {
        let nodes = self.state.nodes.read().await;
        let leader = self.state.leader.read().await.clone();
        let scheduler = self.state.scheduler_state.read().await;

        ClusterStateInfo {
            cluster_name: self.state.config.cluster_name.clone(),
            node_count: nodes.len(),
            ready_nodes: nodes
                .values()
                .filter(|n| n.status == NodeStatus::Ready)
                .count(),
            leader,
            pending_pods: scheduler.pending_pods.len(),
            scheduled_pods: scheduler.scheduled_pods.len(),
            nodes: nodes.values().cloned().collect(),
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<ClusterEvent> {
        self.state.event_sender.subscribe()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStateInfo {
    pub cluster_name: String,
    pub node_count: usize,
    pub ready_nodes: usize,
    pub leader: Option<String>,
    pub pending_pods: usize,
    pub scheduled_pods: usize,
    pub nodes: Vec<NodeInfo>,
}

impl Default for NodeResources {
    fn default() -> Self {
        Self {
            cpu_total: 8.0,
            cpu_allocated: 0.0,
            cpu_available: 8.0,
            memory_total: 8 * 1024 * 1024 * 1024,
            memory_allocated: 0,
            memory_available: 8 * 1024 * 1024 * 1024,
            disk_total: 100 * 1024 * 1024 * 1024,
            disk_allocated: 0,
            disk_available: 100 * 1024 * 1024 * 1024,
            network_rx: 0,
            network_tx: 0,
            containers_running: 0,
            containers_total: 0,
        }
    }
}

impl ClusterManager {
    pub fn is_leader(&self) -> bool {
        let leader = self.state.leader.blocking_read();
        leader.as_ref() == Some(&self.state.config.node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_cluster_config() {
        let config = ClusterConfig::default();
        assert!(config.enabled);
        assert!(!config.node_id.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_scheduler_config() {
        let config = SchedulerConfig::default();
        assert!(config.enabled);
        assert_eq!(config.algorithm, SchedulingAlgorithm::BinPacking);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_cluster_creation() {
        let config = ClusterConfig::default();
        let manager = ClusterManager::new(config);

        let state = manager.get_cluster_state().await;
        assert_eq!(state.node_count, 1);
    }
}
