# Module map — what every file says it is

This map lists every `//!` module doc and the first doc line of every public item.
The map harvests both from the source code and paraphrases nothing. When a file has no
doc comment, the map says so. A missing description is a visible gap, not a silent absence.


# crates/pn-gateway


## `crates/pn-gateway/benches/benchmarks.rs`  249 lines

```
//! Performance benchmarks for pn-gateway
```

## `crates/pn-gateway/src/config.rs`  771 lines

```
//! Gateway configuration
```

Public items - first doc line, `-` when undocumented:

- `struct GatewayConfig`: -
- `struct ProotConfig`: -
- `struct BindMount`: -
- `struct DistroConfig`: -
- `struct DistroSource`: -
- `struct CompatConfig`: -
- `struct FailSafeConfig`: -
- `struct CircuitBreakerConfig`: -
- `enum CircuitBreakerState`: -
- `enum FailSafeAction`: -
- `struct GuardrailConfig`: -
- `struct GuardrailRule`: -
- `enum GuardrailAction`: -
- `enum Severity`: -
- `struct HealthConfig`: -
- `struct HealthCheck`: -
- `enum HealthCheckType`: -
- `enum RecoveryAction`: -
- `struct MetricsConfig`: -
- `enum MetricsExporter`: -
- `struct NetworkConfig`: -
- `struct PortMapping`: -
- `struct StorageConfig`: -
- `struct LoggingConfig`: -
- `struct FileRotationConfig`: -
- `enum GatewayState`: -
- `struct GatewayConfigBuilder`: Builder for GatewayConfig
- `fn new`: -
- `fn name`: -
- `fn version`: -
- `fn proot_config`: -
- `fn distro_config`: -
- `fn compat_config`: -
- `fn fail_safe`: -
- `fn guardrails`: -
- `fn health`: -
- `fn metrics`: -
- `fn network`: -
- `fn storage`: -
- `fn logging`: -
- `fn build`: -

## `crates/pn-gateway/src/config_loader.rs`  96 lines

```
//! TOML loading for [`GatewayConfig`].
//!
//! A PDN configuration file namespaces its gateway settings under a top-level
//! `[gateway]` table (see `docs/src/configuration.md`). The same global file
//! also carries unrelated sections (`[general]`, `[grpc]`, `[api]`, ...) that
//! the gateway does not own, so the top level here is intentionally permissive
//! while the `[gateway]` table itself stays strict: every struct in
//! [`crate::config`] is `#[serde(deny_unknown_fields)]`, so an unknown key
//! anywhere under `[gateway]` is rejected with the offending key and its
//! line/column named.
//!
//! Durations are written as strings (`"30s"`, `"500ms"`, `"2m"`) by way of
//! [`crate::duration_fmt`], which also keeps the native `{ secs, nanos }` table
//! and bare integer seconds loadable. [`GatewayConfig::to_toml_str`] writes the
//! same form back out, so a loaded configuration round-trips.
```

Public items - first doc line, `-` when undocumented:

- `enum ConfigError`: -
- `fn from_toml_str`: Parse the `[gateway]` table of a PDN config file from a TOML string
- `const CONFIG_ENV`: Environment variable that names the gateway configuration file
- `fn to_toml_str`: Render this configuration back into the documented TOML shape
- `fn from_toml_file`: Load the `[gateway]` table of a PDN config file from an explicit path

## `crates/pn-gateway/src/duration_fmt.rs`  204 lines

```
//! Human-readable durations in configuration files.
//!
//! `std::time::Duration`'s own serde representation is a struct -
//! `{ secs = 60, nanos = 0 }` - but `docs/src/configuration.md` documents the
//! form every operator expects:
//!
//! ```toml
//! gateway_timeout = "30s"
//! retry_delay = "500ms"
//! ```
//!
//! Until this module existed those documented values could not be loaded at
//! all: `Duration` failed to deserialise a string, and because every config
//! struct is `#[serde(deny_unknown_fields)]` the failure was loud but the
//! documented configuration was still unusable. See issue #34.
//!
//! Accepted inputs, deliberately chosen to match the documentation and common
//! practice rather than to be clever:
//!
//! | input | meaning |
//! |---|---|
//! | `"30s"`, `"30sec"`, `"30secs"`, `"30second"`, `"30seconds"` | seconds |
//! | `"500ms"`, `"500millis"` | milliseconds |
//! | `"2m"`, `"2min"`, `"2mins"`, `"2minute"`, `"2minutes"` | minutes |
//! | `"1h"`, `"1hr"`, `"1hour"`, `"1hours"` | hours |
//! | `"1d"`, `"1day"`, `"1days"` | days |
//! | `30` (bare integer) | seconds, for TOML files written before this existed |
//! | `{ secs = 30, nanos = 0 }` | the native `Duration` form, still accepted so no existing file breaks |
//!
//! Whitespace around the value and between the number and the unit is ignored
//! (`" 30 s "` works). Anything else is an error that names the offending value
//! and the accepted forms - a silent fallback to zero would turn a typo into a
//! disabled timeout, which is the failure mode this whole area is trying to
//! stop.
```

Public items - first doc line, `-` when undocumented:

- `fn parse_duration`: Parse a duration written the way the docs write it
- `fn format_duration`: Render a duration the way the docs write it, so a config round-trips.
- `fn deserialize`: `#[serde(with = "crate::duration_fmt")]` for `Duration` fields.
- `fn serialize`: `#[serde(with = "crate::duration_fmt")]` for `Duration` fields.

## `crates/pn-gateway/src/fail_safe.rs`  201 lines

```
//! Fail-safe manager for PN Gateway
```

Public items - first doc line, `-` when undocumented:

- `struct FailSafeManager`: Fail-safe manager for handling failures gracefully
- `fn new`: -
- `fn config`: -
- `fn execute_with_failsafe`: -
- `fn is_circuit_open`: -
- `fn increment_retry_count`: -
- `fn reset_retry_count`: -
- `fn execute_fallback_actions`: -
- `fn record_failure`: -
- `fn record_success`: -
- `fn get_circuit_state`: -

## `crates/pn-gateway/src/gateway.rs`  178 lines

```
//! Gateway module for PN Gateway
```

Public items - first doc line, `-` when undocumented:

- `struct PnGateway`: PN Gateway - orchestrates PRoot, PRoot-distro, and proot-compat
- `fn new`: -
- `fn initialize`: -
- `fn run_container`: -
- `fn shutdown`: -
- `fn get_state`: -
- `fn get_health`: -
- `fn get_metrics`: -

## `crates/pn-gateway/src/guardrails.rs`  411 lines

```
//! Guardrail manager for PN Gateway
```

Public items - first doc line, `-` when undocumented:

- `struct GuardrailManager`: Guardrail manager for enforcing safety rules
- `fn new`: -
- `fn check_all`: -
- `fn check_condition`: -
- `fn get_violations`: -
- `fn clear_violations`: -
- `struct ViolationRecord`: -

## `crates/pn-gateway/src/health.rs`  370 lines

```
//! Health monitoring for PN Gateway
```

Public items - first doc line, `-` when undocumented:

- `enum HealthCheckHandlerType`: -
- `struct HealthMonitor`: Health monitor for PN Gateway
- `fn new`: -
- `fn start`: -
- `fn stop`: -
- `fn is_healthy`: -
- `fn get_status`: -
- `fn run_check`: -
- `struct HealthStatus`: -
- `fn is_healthy`: -
- `enum OverallHealthStatus`: -
- `enum HealthCheckStatus`: -
- `struct HealthCheckResult`: -

## `crates/pn-gateway/src/lib.rs`  37 lines

```
//! PN Gateway - Bridge layer between PRoot, PRoot-distro, and proot-compat
//!
//! This crate provides the gateway layer that connects PRoot core,
//! PRoot-distro container management, and proot-compat compatibility layer.
//! It provides fail-safe mechanisms and guardrails for robust operation.
```

## `crates/pn-gateway/src/metrics.rs`  300 lines

```
//! Metrics collector for PN Gateway
```

Public items - first doc line, `-` when undocumented:

- `struct MetricsCollector`: Metrics collector for PN Gateway
- `fn new`: -
- `fn start`: -
- `fn stop`: -
- `fn increment_counter`: -
- `fn set_gauge`: -
- `fn record_histogram`: -
- `fn get_metrics`: -
- `fn increment_containers_created`: -
- `fn increment_containers_destroyed`: -
- `fn increment_containers_started`: -
- `fn increment_containers_stopped`: -
- `fn increment_containers_failed`: -
- `fn set_active_containers`: -
- `fn record_container_startup_time`: -
- `fn record_container_memory_usage`: -
- `fn record_container_cpu_usage`: -
- `fn record_gateway_uptime`: -
- `fn record_gateway_memory_usage`: -
- `fn record_gateway_cpu_usage`: -
- `fn record_api_request`: -
- `fn record_error`: -

## `crates/pn-gateway/tests/config_loader.rs`  254 lines

```
//! Integration tests for the gateway TOML loader (issue #27 follow-up).
//!
//! These exercise the *file-shaped* config: a global `config.toml` whose
//! gateway settings live under `[gateway]`.
```

## `crates/pn-gateway/tests/doc_key_drift.rs`  484 lines

```
//! Whole-file documentation ↔ configuration drift guard.
//!
//! ## Scope (honest, read this)
//!
//! This test scans the fenced ```toml blocks of **every** documentation file
//! listed in [`DOC_FILES`], which is every file under `docs/src/` that contains a
//! TOML block. A doc file that is not in `DOC_FILES` is not checked, so adding a
//! new file with TOML blocks means adding it there.
//!
//! `docs/src/architecture.md` contains no ```toml blocks (its config sketches
//! are ```rust), so it contributes zero keys here. Its prose and type-sketch
//! reconciliation was done by hand and is **not** covered by this guard.
//!
//! ## What counts as a "documented key"
//!
//! A block that documents another tool's configuration (containerd, an OCI
//! runtime spec, the previous pn-next schema) carries a ``# not-pdn-config:``
//! comment and is skipped entirely: its keys are not this crate's to own.
//!
//! A bare `key = ...` assignment inside a fenced ```toml block — quoted keys
//! (`"org.pdn.type" = ...`) are dynamic map entries, not config field names.
//! The bare sub-keys of an inline-table value (`circuit_breaker = { a = 1 }`)
//! are keys too. An assignment whose enclosing table is a dynamic map (`env`,
//! `labels`, `annotations`, `mounts`, `volumes`, `tmpfs`, `devices`, `sysctls`,
//! `ulimits`, `rules`) is a map entry, not a key, and is skipped. A
//! `# planned: k1, k2` comment line declares the listed keys as keys too.
//!
//! ## Verdict rule
//!
//! Every documented key must either
//!   1. exist as a field name of a deserialisable config struct reachable from
//!      this crate (the real field set is derived by serialising the structs —
//!      never hand-copied), or
//!   2. be listed in [`PLANNED`] below, with a reason.
//!
//! ## Ceiling (deliberate)
//!
//! The guard matches key **names**, not full nesting paths. A documented key
//! whose leaf name happens to match a real field is accepted even if its
//! section path is wrong (e.g. a `[network.config] subnet` that should be a
```

# crates/pn-next


## `crates/pn-next/benches/benchmarks.rs`  276 lines

```
//! Performance benchmarks for pn-next
```

## `crates/pn-next/src/api.rs`  1422 lines

```
//! API server for PN Next
//!
//! Provides a RESTful API for container management with real integration
//! to proot-distro-core and proot-core.
```

Public items - first doc line, `-` when undocumented:

- `struct ApiConfig`: -
- `struct ApiServer`: API server with real backend integration
- `fn new`: -
- `fn start`: -
- `fn stop`: -
- `struct ApiState`: -
- `fn new`: -
- `enum ApiError`: -

## `crates/pn-next/src/backup_restore/mod.rs`  1617 lines

```
//! Backup/Restore Module
//! Container snapshots and disaster recovery for PDN
```

Public items - first doc line, `-` when undocumented:

- `struct ContainerInfo`: -
- `struct BackupConfig`: -
- `struct RetentionPolicy`: -
- `struct BackupHooks`: -
- `struct BackupMetadata`: -
- `enum BackupType`: -
- `enum BackupStatus`: -
- `struct BackupManifest`: -
- `struct ManifestEntry`: -
- `enum FileType`: -
- `struct VolumeInfo`: -
- `struct ContainerConfigSnapshot`: -
- `struct MountSpec`: -
- `struct PortSpec`: -
- `struct ResourceLimits`: -
- `struct LogsSnapshot`: -
- `struct RestoreOptions`: -
- `enum BackupEvent`: -
- `struct BackupManager`: Backup manager
- `struct BackupProgress`: -
- `fn new`: -
- `fn start`: -
- `fn create_backup`: -
- `fn restore_backup`: -
- `fn list_backups`: -
- `fn get_backup`: -
- `fn delete_backup`: -
- `fn cancel_backup`: -
- `fn get_backup_progress`: -
- `fn load_backups`: -
- `fn subscribe_events`: -

## `crates/pn-next/src/cgroup_view.rs`  369 lines

```
//! A private cgroupfs view (adoption S8 of `research/DISSECTION_SHROOT.md`).
//!
//! The reference is `native/shroot/src/cgroup.c` at commit `7ab99d6`. That file is not
//! enforcement: it is a userspace *view* of cgroupfs. It serves a private file enum
//! (`PROCS, THREADS, EVENTS, CONTROLLERS, SUBTREE, TYPE, FREEZE, ALLOW, DENY, LIST, TASKS,
//! NOTIFY, RELEASE, PIDS_MAX, PIDS_CURRENT, PIDS_EVENTS`, source lines 25-26) with per-file
//! modes 0644/0444 (`:31-33`), answers `mkdir` and subtree writes itself (`md_cgroup_mkdir`
//! at `:755`, `subtree_write` at `:872`), and produces its own errno values (`-EBUSY`,
//! `-EPERM`, `-EACCES`, `-EROFS` at `:781-986`) for what it will not honour. Nothing in it
//! writes to the host's `/sys/fs/cgroup`. Its own documentation states the ceiling plainly
//! (`docs/shroot.md:75`): hierarchical `pids.max`, membership, freezer and sampled CPU/RSS
//! counters are served, while "cgroup CPU, cpuset, memory and device policies are not
//! enforced" and "sampled RSS is not kernel page charging".
//!
//! PDN had the other shape, and the shape outlived the first fix. `supervisor.rs` wrote
//! `cgroup.procs`, `cpu.max` and `memory.max` into an ordinary directory and returned `Ok()`
//! (WORKLIST A20): the write succeeded, the caller believed a limit was in force, and no
//! kernel limit existed. Wave 5 named the skip (A20 v2) rather than refusing, but a limit was
//! still *represented by files*, and the only reason it was not faked here was that a probe
//! happened to fail. This module is the structural fix: one view with a named verdict per
//! knob, answering reads from the platform (`/proc`, the only source that is true on a host
//! where cgroupfs is not writable) and refusing the rest by errno, so "a limit is in force"
//! is never something a caller can infer from a completed write.
```

Public items - first doc line, `-` when undocumented:

- `enum CgroupFile`: -
- `const fn`: The file's name in the cgroupfs the view presents.
- `const fn`: Whether the file is a resource *policy*, that is, a file the kernel must enforce for the written value to have meaning
- `enum CgroupErrno`: -
- `const fn`: The errno as the kernel names it.
- `const fn`: The negative Linux errno, so a caller can report the same number as the kernel.
- `struct CgroupRefusal`: -
- `struct CgroupView`: -
- `fn for_group`: A view for a group named `name`, optionally under a configured `mount`
- `fn with_group`: A view bound to an explicit group directory
- `fn group`: The backing group directory, when the view has a real one.
- `fn add_pid`: Record a member: the view's answer for `cgroup.procs`.
- `fn read`: Read a knob
- `fn write`: Write a knob

## `crates/pn-next/src/cluster/mod.rs`  889 lines

```
//! Multi-node Clustering Module
//! Distributed scheduling and cluster management for PDN
```

Public items - first doc line, `-` when undocumented:

- `struct ClusterConfig`: -
- `struct SchedulerConfig`: -
- `enum SchedulingAlgorithm`: -
- `struct GossipConfig`: -
- `struct NodeInfo`: -
- `enum NodeStatus`: -
- `enum NodeRole`: -
- `struct NodeResources`: -
- `struct ClusterState`: -
- `struct SchedulerState`: -
- `struct PodSpec`: -
- `struct ContainerSpec`: -
- `struct ResourceRequirements`: -
- `struct Affinity`: -
- `struct NodeAffinity`: -
- `struct PodAffinity`: -
- `struct PodAntiAffinity`: -
- `struct NodeSelectorTerm`: -
- `struct NodeSelectorRequirement`: -
- `enum NodeSelectorOperator`: -
- `struct PreferredSchedulingTerm`: -
- `struct PodAffinityTerm`: -
- `struct WeightedPodAffinityTerm`: -
- `struct LabelSelector`: -
- `struct LabelSelectorRequirement`: -
- `enum LabelSelectorOperator`: -
- `struct Toleration`: -
- `enum TolerationOperator`: -
- `enum TaintEffect`: -
- `struct PortSpec`: -
- `struct EnvVar`: -
- `struct VolumeMount`: -
- `enum ClusterEvent`: -
- `struct ClusterManager`: Cluster manager
- `fn new`: -
- `fn start`: -
- `fn join_cluster`: -
- `fn submit_pod`: -
- `fn get_cluster_state`: -
- `fn subscribe_events`: -
- `struct ClusterStateInfo`: -
- `fn is_leader`: -

## `crates/pn-next/src/container_socket_activation/mod.rs`  612 lines

```
//! Socket Activation for Containers Module
//! Extends socket activation to support on-demand container startup
```

Public items - first doc line, `-` when undocumented:

- `struct ContainerSocketConfig`: -
- `struct HealthCheckConfig`: -
- `enum HealthCheckType`: -
- `enum ContainerSocketEvent`: -
- `struct ContainerSocketManager`: Container socket activation manager
- `fn new`: Create a new container socket manager
- `fn with_runtime`: Set container runtime
- `fn with_events`: Set event broadcaster
- `fn subscribe_events`: Get event receiver
- `fn register_container_socket`: Register a container socket activation
- `fn unregister_container_socket`: Unregister a container socket
- `fn handle_connection`: Handle an incoming connection. The base manager calls this function.
- `fn get_active`: Get active container info
- `fn list_container_sockets`: List all container sockets

## `crates/pn-next/src/cri_plugin/mod.rs`  177 lines

```
//! Kubernetes CRI Plugin Module (Stub Implementation)
//! Provides CRI (Container Runtime Interface) types and basic structure for containerd integration
```

Public items - first doc line, `-` when undocumented:

- `struct CRIPluginConfig`: -
- `struct CRIPluginManager`: CRI Plugin manager, a stub implementation.
- `fn new`: -
- `fn start`: -
- `fn stop`: -
- `fn get_config`: -
- `struct ContainerSpec`: -
- `struct PodSandboxSpec`: -
- `struct SandboxMetadata`: -
- `struct DNSConfig`: -
- `struct PortMapping`: -
- `enum Protocol`: -
- `struct EnvVar`: -
- `struct Mount`: -
- `enum MountPropagation`: -

## `crates/pn-next/src/daemon_runtime/mod.rs`  1139 lines

```
//! Daemon Runtime - Independent lifecycle manager for services
//!
//! Provides a standalone daemon process that manages service lifecycle
//! independently of the main PDN process. Supports:
//! - Service registration and discovery
//! - Independent lifecycle (survives container restarts)
//! - State persistence and recovery
//! - Health monitoring and auto-recovery
//! - IPC communication via Unix sockets
```

Public items - first doc line, `-` when undocumented:

- `struct DaemonRuntimeConfig`: -
- `struct ManagedService`: -
- `enum DaemonRestartPolicy`: -
- `struct DaemonHealthCheckConfig`: -
- `struct DaemonServiceState`: -
- `enum DaemonServiceStatus`: -
- `enum DaemonHealthStatus`: -
- `enum DaemonEvent`: -
- `enum DaemonMessage`: -
- `enum DaemonResponse`: -
- `struct DaemonRuntime`: Daemon runtime
- `fn new`: -
- `fn command_sender`: -
- `fn subscribe_events`: -
- `fn start`: Start every daemon task
- `fn register_service`: -
- `fn unregister_service`: -
- `fn start_service`: -
- `fn stop_service`: -
- `fn restart_service`: -
- `fn get_service_status`: -
- `fn list_services`: -
- `fn get_daemon_status`: -
- `fn shutdown`: -
- `struct DaemonStatus`: -

## `crates/pn-next/src/dbus/mod.rs`  1370 lines

```
//! D-Bus Broker - Inter-container IPC and service discovery
//!
//! Provides a lightweight D-Bus message broker for communication between
//! container services and the supervisor. Implements a subset of the D-Bus
//! specification suitable for Termux/Android environments.
```

Public items - first doc line, `-` when undocumented:

- `enum MessageType`: -
- `struct MessageHeader`: -
- `struct DBusMessage`: -
- `struct ServiceRegistration`: -
- `struct ConnectionInfo`: -
- `struct DBusBrokerConfig`: -
- `struct DBusBroker`: Main D-Bus broker
- `enum DBusEvent`: -
- `fn new`: Create a new D-Bus broker
- `fn subscribe_events`: Get event receiver
- `fn start`: Start the broker listener
- `fn with_supervisor`: Attach the supervisor that `org.pnnext.Supervisor` calls act on
- `fn register_service`: Register a well-known service name
- `fn unregister_service`: Unregister a service
- `fn lookup_service`: Look up a service
- `fn list_services`: List all services

## `crates/pn-next/src/dbus/services.rs`  507 lines

```
//! D-Bus Service Definitions for PN Next Inter-Container Communication
//!
//! This module defines the standard D-Bus interfaces and service definitions
//! used for communication between containers and the supervisor.
```

Public items - first doc line, `-` when undocumented:

- `const SUPERVISOR`: PN Next Supervisor service
- `const CONTAINER_MANAGER`: PN Next Container Manager service
- `const NETWORK_MANAGER`: PN Next Network Manager service
- `const STORAGE_MANAGER`: PN Next Storage Manager service
- `const CLUSTER_MANAGER`: PN Next Cluster Manager service
- `const METRICS`: PN Next Metrics service
- `const EVENT_BUS`: PN Next Event Bus service
- `const SUPERVISOR`: -
- `const CONTAINER_MANAGER`: -
- `const NETWORK_MANAGER`: -
- `const STORAGE_MANAGER`: -
- `const CLUSTER_MANAGER`: -
- `const METRICS`: -
- `const EVENT_BUS`: -
- `const SUPERVISOR`: -
- `const CONTAINER_MANAGER`: -
- `const NETWORK_MANAGER`: -
- `const STORAGE_MANAGER`: -
- `const CLUSTER_MANAGER`: -
- `const METRICS`: -
- `const EVENT_BUS`: -
- `const CONTAINER`: -
- `const SERVICE`: -
- `const SUPERVISOR`: Supervisor service introspection
- `const CONTAINER_MANAGER`: Container Manager service introspection
- `const NETWORK_MANAGER`: Network Manager service introspection
- `const STORAGE_MANAGER`: Storage Manager service introspection
- `const EVENT_BUS`: Event Bus service introspection
- `const METRICS`: Metrics service introspection
- `fn get_introspection`: Get introspection XML for a service
- `struct ContainerConfig`: -
- `struct MountConfig`: -
- `struct ResourceLimits`: -
- `struct NetworkConfig`: -
- `struct VolumeConfig`: -
- `struct ServiceStatus`: -
- `struct ContainerInfo`: -
- `struct NetworkInfo`: -
- `struct VolumeInfo`: -

## `crates/pn-next/src/dbus/zbus_client.rs`  688 lines

```
//! ZBus-based D-Bus Client for Inter-Container Communication
//!
//! This module provides a proper D-Bus client implementation using zbus
//! for communicating with the D-Bus broker and other containers.
```

Public items - first doc line, `-` when undocumented:

- `struct ZBusClient`: ZBus-based D-Bus client for container communication
- `fn new`: Create a new ZBus client
- `fn connect`: Connect to the D-Bus broker via Unix socket
- `fn connect_session`: Connect to the session bus for container-to-container communication.
- `fn connect_system`: Connect to the system bus
- `fn unique_name`: Get the unique name assigned by the bus
- `fn request_name`: Request a well-known name on the bus
- `fn release_name`: Release a well-known name
- `fn call_method`: Call a method on a remote service with a single argument tuple
- `fn call_method_no_args`: Call a method with no arguments
- `fn subscribe_signals`: Subscribe to signals from a service
- `fn get_proxy`: Get a cached proxy for a well-known service.
- `fn list_names`: List all names on the bus
- `fn name_has_owner`: Check if a name has an owner
- `fn ping`: Ping the bus
- `fn get_machine_id`: Get machine ID
- `fn introspect`: Introspect a remote object
- `fn disconnect`: Disconnect from the bus
- `struct ContainerDBusClient`: Container D-Bus client for inter-container communication
- `fn new`: Create a new container D-Bus client
- `fn connect`: Connect to the broker
- `fn register_container_service`: Register this container as a service
- `fn start_service`: Call supervisor to start a service
- `fn stop_service`: Call supervisor to stop a service
- `fn get_service_status`: Get service status from supervisor
- `fn create_container`: Create a container via container manager
- `fn start_container`: Start a container
- `fn stop_container`: Stop a container
- `fn list_containers`: List all containers
- `fn exec_in_container`: Execute command in container
- `fn publish_event`: Publish an event to the event bus
- `fn subscribe_events`: Subscribe to events
- `fn client`: Get underlying client for advanced usage
- `fn client_mut`: Get mutable underlying client
- `fn create_container_connection`: Helper to create a D-Bus connection for a container

## `crates/pn-next/src/gitops/mod.rs`  1067 lines

```
//! GitOps Integration Module
//! Flux/ArgoCD style GitOps integration for PDN
```

Public items - first doc line, `-` when undocumented:

- `struct EnvVar`: -
- `struct GitOpsConfig`: -
- `struct GitCredentials`: -
- `enum ResourceType`: -
- `struct GitResource`: -
- `struct ResourceMetadata`: -
- `struct OwnerReference`: -
- `enum SyncStatus`: -
- `struct Application`: -
- `struct ApplicationSource`: -
- `struct HelmSource`: -
- `struct HelmParameter`: -
- `struct KustomizeSource`: -
- `struct DirectorySource`: -
- `struct JsonnetSource`: -
- `struct PluginSource`: -
- `struct ApplicationDestination`: -
- `struct SyncPolicy`: -
- `struct SyncPolicyAutomated`: -
- `struct RetryStrategy`: -
- `struct Backoff`: -
- `struct ResourceIgnoreDifference`: -
- `struct Info`: -
- `struct ApplicationStatus`: -
- `struct SyncStatusInfo`: -
- `struct ComparedTo`: -
- `struct HealthStatusInfo`: -
- `enum HealthStatus`: -
- `struct Condition`: -
- `struct OperationState`: -
- `struct SyncResult`: -
- `struct ResourceResult`: -
- `struct HookResult`: -
- `struct SyncHistory`: -
- `struct ResourceStatus`: -
- `enum GitOpsEvent`: -
- `struct GitOpsManager`: GitOps manager
- `fn new`: -
- `fn applications_dir`: The directory applications are read from: `<repo>/applications`.
- `fn load_from_disk`: Read applications from `<repo>/applications/*.yaml`
- `fn load_state`: Restore the state written by a previous invocation.
- `fn save_state`: Persist the current state for the next invocation.
- `fn config`: -
- `fn repo_path`: -
- `fn list_applications`: -
- `fn get_application`: -
- `fn sync_now`: Sync every loaded application, returning how many synced cleanly.
- `fn start`: -
- `fn sync_one`: Sync one named application, taking `&self` so callers hold a manager.

## `crates/pn-next/src/lib.rs`  351 lines

```
//! PN Next - Advanced architecture for PRoot Distro Next
//!
//! This crate provides the advanced architecture layer that builds upon
//! PRoot, PRoot-distro, and proot-compat to provide a more sophisticated,
//! user-friendly, and organized system for managing containers on Termux.
```

Public items - first doc line, `-` when undocumented:

- `struct PnNext`: PN Next - Advanced architecture for PRoot Distro Next
- `fn new`: -
- `fn initialize`: -
- `fn run_container`: -
- `fn start_api`: -
- `fn shutdown`: -
- `fn get_state`: -
- `struct PnNextConfig`: -
- `enum PnNextState`: -

## `crates/pn-next/src/multiarch_build/mod.rs`  1141 lines

```
//! Multi-arch Build Service Module
//! Cross-compilation and multi-architecture build service for PDN
```

Public items - first doc line, `-` when undocumented:

- `struct BuildServiceConfig`: -
- `enum TargetArch`: -
- `fn docker_platform`: -
- `fn rust_target`: -
- `fn go_arch`: -
- `struct RegistryConfig`: -
- `struct CacheConfig`: -
- `struct BuildHooks`: -
- `struct BuildResourceLimits`: -
- `enum BuildStatus`: -
- `struct BuildSpec`: -
- `enum BuildSource`: -
- `struct BuildSecret`: -
- `enum SecretSource`: -
- `struct BuildOutput`: -
- `enum OutputType`: -
- `struct BuildMetadata`: -
- `struct BuildProgress`: -
- `struct LogEntry`: -
- `struct BuildArtifact`: -
- `struct PlatformResult`: -
- `struct LayerInfo`: -
- `enum BuildEvent`: -
- `struct BuildManager`: Build manager
- `fn new`: -
- `fn start`: -
- `fn submit_build`: -
- `fn get_build`: -
- `fn list_builds`: -
- `fn cancel_build`: -
- `fn retry_build`: -
- `fn get_build_logs`: -
- `fn get_artifacts`: -
- `fn subscribe_events`: -
- `fn get_config`: -

## `crates/pn-next/src/network.rs`  705 lines

```
//! Network management for PN Next
//!
//! Provides network management with CNI (Container Network Interface) plugin support
//! for container networking.
```

Public items - first doc line, `-` when undocumented:

- `struct NetworkConfig`: -
- `struct CniConfig`: -
- `struct CniPluginConfig`: -
- `struct CniNetworkConfig`: -
- `struct CniIpamConfig`: -
- `struct CniRoute`: -
- `struct CniRange`: -
- `struct CniRuntimeArgs`: -
- `struct NetworkManager`: Network manager with CNI support
- `fn new`: -
- `fn initialize`: -
- `fn shutdown`: -
- `fn create_network`: -
- `fn remove_network`: -
- `fn get_network`: -
- `fn list_networks`: -
- `fn connect_container`: Connect a container to a network using CNI
- `fn disconnect_container`: Disconnect container from network
- `fn get_container_interface`: Get container network interface
- `fn list_interfaces`: List all interfaces
- `struct CniResult`: -
- `struct CniInterface`: -
- `struct CniIp`: -
- `struct CniDns`: -
- `struct Network`: -
- `struct NetworkInterface`: -

## `crates/pn-next/src/plugin.rs`  493 lines

```
//! Plugin system for PN Next
//!
//! Provides dynamic plugin loading via shared libraries (.so/.dylib/.dll)
//! with a hook system for extensibility.
```

Public items - first doc line, `-` when undocumented:

- `struct PluginConfig`: -
- `struct PluginManager`: Plugin manager
- `fn new`: -
- `fn load_plugins`: -
- `fn unload_all`: -
- `fn register_plugin`: -
- `fn unregister_plugin`: -
- `fn get_plugin_info`: -
- `fn register_hook`: -
- `fn execute_hook`: -
- `fn list_plugins`: -
- `trait Plugin`: -
- `struct PluginInfo`: -
- `struct PluginContext`: Plugin context provided to plugins during initialization
- `trait PluginLogger`: -
- `trait PluginApi`: -
- `enum PluginEvent`: -
- `trait Hook`: -
- `struct HookContext`: -
- `struct ExamplePlugin`: Built-in plugin example

## `crates/pn-next/src/policy_engine/mod.rs`  758 lines

```
//! Policy Engine Module
//! Declarative policy evaluation for service management decisions
```

Public items - first doc line, `-` when undocumented:

- `struct PolicyEngineConfig`: -
- `enum PolicyAction`: -
- `struct PolicyEffect`: -
- `struct PolicyRule`: -
- `enum PolicyCondition`: -
- `enum ComparisonOperator`: -
- `enum ResourceType`: -
- `enum HealthStatus`: -
- `struct EvaluationContext`: -
- `struct EvaluationResult`: -
- `struct PolicyEngine`: Policy engine
- `fn new`: Create a new policy engine
- `fn load_policies`: Load policies from directory
- `fn add_rule`: Add a policy rule
- `fn remove_rule`: Remove a policy rule
- `fn set_rule_enabled`: Enable/disable a rule
- `fn evaluate`: Evaluate policies for a context
- `fn get_rules`: Get all rules
- `fn get_rule`: Get rule by name
- `fn list_rules_by_tag`: List rules by tag
- `fn start_evaluation_loop`: Start policy evaluation loop
- `fn export_policies`: Export policies to file
- `fn production_safety`: Production safety policies
- `fn development`: Development policies

## `crates/pn-next/src/readiness.rs`  186 lines

```
//! Start coordination as one byte on a pipe (WORKLIST C10).
//!
//! runc's init process tells its parent it has finished setting up by writing a
//! single byte to a pipe the parent holds the read end of
//! (`libcontainer/standard_init_linux.go`: `fifoFile.Write([]byte("0"))`), and
//! the parent's start waits for exactly that
//! (`libcontainer/container_linux.go`). The child's readiness is an *event*: a
//! parent that never sees it knows the start failed.
//!
//! PDN had the other shape. `podnext service start` spawned the process and then
//! polled the reported state for up to 30 x 500 ms, printing
//! `start initiated (current state: ...)` and exiting 0 when the poll ran out -
//! a service that never came up was reported as a start. This module is the
//! port: the child sees the write end at [`READY_FD_NUM`] as `$PN_READY_FD`, the
//! parent blocks on the read end for a bounded time, and running out of that
//! time is a named failure ([`ReadinessError::Timeout`]) rather than silence.
//!
//! The write end is installed with `dup2` in the child (a `pre_exec` step) and
//! stays close-on-exec in the parent, so it is not inherited by any *other*
//! child the same process spawns. Clearing `FD_CLOEXEC` instead would leak the
//! write end into a sibling spawn, which would then hold the pipe open and hide
//! the EOF that says "the child died before signalling".
```

Public items - first doc line, `-` when undocumented:

- `const READY_FD_ENV`: Environment variable carrying the write end's descriptor to the child.
- `const READY_FD_NUM`: The descriptor the child sees the pipe's write end at: 0/1/2 are stdio, so the first free number is where runc's own extra files land too.
- `enum ReadinessError`: -
- `struct ReadinessPipe`: The parent's read end and the child's write end. Both ends are close-on-exec until [`ReadinessPipe::install`] dup's the write end into the child.
- `fn new`: Create the pipe, close-on-exec on both ends.
- `fn install`: Give `cmd`'s child the write end at [`READY_FD_NUM`] and the matching `$PN_READY_FD`
- `fn close_child_end`: Close the parent's copy of the write end, after the spawn.
- `fn wait`: Wait, bounded, for the one readiness byte

## `crates/pn-next/src/resource_quota/mod.rs`  838 lines

```
//! Resource Quota Module
//! Enforces resource limits independent of container limits via supervisor
```

Public items - first doc line, `-` when undocumented:

- `struct ResourceQuotaConfig`: -
- `struct ResourceQuotas`: -
- `enum QuotaViolationAction`: -
- `struct ResourceUsage`: -
- `struct QuotaViolation`: -
- `enum ResourceType`: -
- `struct ResourceQuotaManager`: -
- `fn new`: Create a new resource quota manager
- `fn with_events`: Set event broadcaster
- `fn subscribe_events`: Get event receiver
- `fn get_quotas`: Get quotas for a service
- `fn set_quotas`: Set quotas for a service, persisting the change
- `fn stored_quotas`: The service's own stored quota, or an all-unset one
- `fn all_quotas`: Every configured service quota, for `quota list`
- `fn remove_quotas`: Drop a service's quota and persist the removal.
- `fn load_quotas`: Load persisted service quotas, replacing whatever is in memory
- `fn save_quotas`: Write the current service quotas to disk.
- `fn update_usage`: Update resource usage for a service
- `fn collect_usage`: Get current usage for a service from the system.
- `fn get_usage_history`: Get usage history for a service
- `fn get_violations`: Get recent violations
- `fn get_service_violations`: Get violations for a specific service
- `fn start_monitoring`: Start monitoring loop
- `fn shutdown`: Shutdown

## `crates/pn-next/src/restart_policy.rs`  133 lines

```
//! B14 - the restart decision, as a pure module.
//!
//! `supervisor::RestartPolicy` says what the operator wants. This module says what
//! the supervisor must do next, and it says it over plain values only: no process,
//! no clock, no lock, no `async`. The whole decision table therefore runs in a unit
//! test (`tests/restart_policy.rs`) with no subprocess and no supervisor.
//!
//! The runtime in `supervisor.rs` calls these functions, so the table and the loop
//! cannot drift: one exit code, one manual stop, one window have exactly one verdict.
```

Public items - first doc line, `-` when undocumented:

- `enum ServiceOutcome`: -
- `enum GiveUpReason`: -
- `enum RestartDecision`: -
- `struct RestartInput`: -
- `fn condition_matches`: Does the policy's `condition` accept this outcome?  A manual stop is final for every condition: the reference loop re-checks `user_stopped` after the 
- `fn recent_restarts`: The attempt number this exit is: 1 for the first of a window, else one more than the count already inside it
- `fn backoff_secs`: Exponential backoff capped at `max_delay_secs`: `base * multiplier^attempt`.
- `fn decide`: The whole decision: circuit, condition, budget, then delay

## `crates/pn-next/src/runtime.rs`  1701 lines

```
//! Runtime management and container lifecycle service for PN Next
//!
//! Provides a complete service layer for container lifecycle management
//! with a CRI-compatible runtime API.
```

Public items - first doc line, `-` when undocumented:

- `struct RuntimeConfig`: -
- `struct ResourceLimits`: -
- `enum RuntimeState`: -
- `struct ContainerEvent`: -
- `enum ContainerEventType`: -
- `struct ContainerInfo`: -
- `enum ContainerState`: -
- `struct NetworkInfo`: -
- `struct PortMapping`: -
- `struct MountInfo`: -
- `struct CreateContainerRequest`: -
- `struct SandboxConfig`: -
- `struct DnsConfig`: -
- `struct SandboxInfo`: -
- `enum SandboxState`: -
- `struct CreateSandboxRequest`: -
- `struct StartSandboxRequest`: -
- `struct StopSandboxRequest`: -
- `struct RemoveSandboxRequest`: -
- `struct CheckpointConfig`: -
- `struct RestoreConfig`: -
- `struct CheckpointResult`: -
- `struct RestoreResult`: -
- `struct StartContainerRequest`: -
- `struct StopContainerRequest`: -
- `struct RemoveContainerRequest`: -
- `struct ExecRequest`: -
- `struct ExecResponse`: -
- `struct ContainerStats`: -
- `struct CpuUsage`: -
- `struct MemoryUsage`: -
- `struct NetworkUsage`: -
- `struct BlockIoUsage`: -
- `struct Runtime`: Runtime service - manages container lifecycle
- `fn new`: -
- `fn with_gateway`: -
- `fn initialize`: Initialize the runtime service
- `fn shutdown`: Shutdown the runtime service
- `fn get_state`: Get runtime state
- `fn subscribe_events`: Subscribe to container events
- `fn create_container`: Create a new container
- `fn start_container`: Start a container
- `fn apply_resource_limits`: Apply resource limits to a container through the private cgroupfs view
- `fn remove_resource_limits`: Remove resource limits and clean up the cgroup.
- `fn update_resource_limits`: Update resource limits for a running container
- `fn stop_container`: Stop a container
- `fn remove_container`: Remove a container
- `fn checkpoint_container`: ======================================================================== Checkpoint/Restore Support, CRIU Integration, ===============================
- `fn restore_container`: Restore a container from a checkpoint using CRIU
- `fn list_checkpoints`: List available checkpoints
- `fn delete_checkpoint`: Delete a checkpoint
- `fn is_criu_available`: Check if CRIU is available
- `fn create_sandbox`: -
- `fn start_sandbox`: Start a sandbox, also known as a pod. It creates a shared network namespace.
- `fn stop_sandbox`: Stop a sandbox, also known as a pod.
- `fn remove_sandbox`: Remove a sandbox, also known as a pod.
- `fn get_sandbox`: Get sandbox info
- `fn list_sandboxes`: List all sandboxes
- `fn add_container_to_sandbox`: Add a container to a sandbox, also known as a pod.
- `fn remove_container_from_sandbox`: Remove container from sandbox
- `fn get_container`: Get container info
- `fn list_containers`: List containers with optional filter
- `fn exec_container`: Execute command in container
- `fn get_container_stats`: Get container stats
- `fn version`: Get runtime version info
- `fn status`: Get runtime status
- `struct ContainerFilter`: -
- `struct RuntimeVersion`: -
- `struct RuntimeStatus`: -

## `crates/pn-next/src/scheduler.rs`  194 lines

```
//! Scheduler for PN Next
```

Public items - first doc line, `-` when undocumented:

- `struct SchedulerConfig`: -
- `enum TaskPriority`: -
- `struct Task`: -
- `enum TaskStatus`: -
- `fn new`: -
- `struct Scheduler`: Scheduler for managing tasks
- `fn new`: -
- `fn start`: -
- `fn stop`: -
- `fn submit`: -
- `fn get_task`: -
- `fn cancel_task`: -
- `fn get_queue_size`: -
- `fn get_running_count`: -

## `crates/pn-next/src/security.rs`  151 lines

```
//! Security management for PN Next
```

Public items - first doc line, `-` when undocumented:

- `struct SecurityConfig`: -
- `struct SecurityManager`: Security manager
- `fn new`: -
- `fn initialize`: -
- `fn shutdown`: -
- `fn validate_container_creation`: -
- `fn validate_container_config`: -
- `fn apply_security_profile`: -
- `fn get_capabilities`: -
- `fn set_capabilities`: -
- `struct Policy`: -
- `struct PolicyRule`: -
- `enum PolicyAction`: -
- `struct Capability`: -

## `crates/pn-next/src/socket_activation/mod.rs`  756 lines

```
//! Socket Activation - systemd-style on-demand container/service start
//!
//! Implements socket activation similar to systemd's socket units:
//! - Listen on sockets (Unix, TCP, UDP) and start services on first connection
//! - Pass file descriptors to started processes via FD passing
//! - Support for multiple sockets per service
//! - Integration with ServiceSupervisor for lifecycle management
```

Public items - first doc line, `-` when undocumented:

- `enum SocketType`: -
- `struct SocketConfig`: -
- `struct SocketActivationConfig`: -
- `enum SocketActivationEvent`: -
- `enum ListenerType`: -
- `struct SocketActivationManager`: Socket activation manager
- `fn new`: -
- `fn set_service_supervisor`: -
- `fn subscribe_events`: -
- `fn add_socket`: -
- `fn remove_socket`: -
- `fn start`: -
- `fn list_sockets`: -
- `fn get_socket_status`: -
- `fn shutdown`: -
- `fn load_from_directory`: -

## `crates/pn-next/src/storage.rs`  180 lines

```
//! Storage management for PN Next
```

Public items - first doc line, `-` when undocumented:

- `struct StorageConfig`: -
- `struct StorageManager`: Storage manager
- `fn new`: -
- `fn initialize`: -
- `fn shutdown`: -
- `fn create_volume`: -
- `fn remove_volume`: -
- `fn get_volume`: -
- `fn list_volumes`: -
- `fn mount_volume`: -
- `fn unmount_volume`: -
- `struct Volume`: -
- `trait VolumeDriver`: -

## `crates/pn-next/src/structured_logging/mod.rs`  872 lines

```
//! Structured Logging Module
//! Journald-style logging with rotation, querying, and filtering
```

Public items - first doc line, `-` when undocumented:

- `struct LogEntry`: -
- `enum LogLevel`: -
- `struct LogQuery`: -
- `struct RotationConfig`: -
- `struct StructuredLoggingConfig`: -
- `struct StructuredLogging`: Structured logging backend
- `fn new`: Create a new structured logging backend
- `fn log`: Log a message
- `fn log_with_source`: Log with source location
- `fn query`: Query logs
- `fn get_stats`: Get log statistics
- `fn flush`: Flush buffer immediately
- `fn shutdown`: Shutdown gracefully
- `struct LogStats`: -
- `fn init_global_logger`: Initialize global logger
- `fn global_logger`: Get global logger

## `crates/pn-next/src/supervisor.rs`  2989 lines

```
//! Service Supervisor - runit/s6-style supervisor for container services
//!
//! Provides a process supervisor for managing container services with
//! declarative service definitions, dependency DAG, health checks,
//! and automatic restart policies.
```

Public items - first doc line, `-` when undocumented:

- `struct ServiceConfig`: -
- `struct ServiceMeta`: -
- `enum ServiceType`: -
- `struct ProcessConfig`: -
- `struct ResourceLimits`: -
- `struct RestartPolicy`: -
- `struct CircuitBreaker`: -
- `fn new`: Create a new circuit breaker
- `fn record_failure`: Record a failure and check if the circuit must open.
- `fn record_success`: Record a success
- `fn allows_operation`: Check if circuit allows operation
- `fn reset`: Reset the circuit breaker
- `enum RestartCondition`: -
- `struct HealthCheckConfig`: -
- `struct StdioConfig`: -
- `struct LogRotationConfig`: -
- `struct SocketConfig`: -
- `enum SocketType`: -
- `enum ServiceState`: -
- `struct ServiceStatus`: -
- `enum HealthStatus`: -
- `struct ResourceUsage`: -
- `struct ServiceSupervisor`: Service supervisor managing multiple services
- `struct SupervisorConfig`: -
- `enum SupervisorState`: -
- `enum SupervisorEvent`: -
- `fn writable_cgroupfs`: Find a cgroup v2 mount this process can actually write to, if any
- `fn new`: Create a new service supervisor
- `fn subscribe_events`: Get event receiver
- `fn initialize`: Initialize and load services
- `fn load_services`: Load service definitions from TOML files
- `fn load_service_from_file`: Load a single service from TOML file
- `fn build_dependency_graph`: Build dependency graph from service dependencies
- `fn validate_dependencies`: Validate the dependency graph and reject cycles.
- `fn start_supervision_loop`: Start the supervision loop
- `fn supervision_tick`: Single supervision tick
- `fn check_service_health`: Check service health and handle restarts
- `fn should_restart`: Determine if the service must restart.
- `fn schedule_restart`: Schedule a service restart
- `fn start_service`: -
- `fn run_health_check`: Run a single health check
- `fn stop_service`: -
- `fn restart_service`: Restart a service
- `fn reload_service`: Reload a service. Send SIGHUP to the service.
- `fn get_status`: Get service status
- `fn list_services`: List all services
- `fn get_state`: Get supervisor state
- `fn shutdown`: Shutdown supervisor
- `fn emit`: Emit event
- `fn enable_service`: Enable a service. The service starts at boot.
- `fn disable_service`: Disable a service
- `fn is_enabled`: Check if service is enabled
- `fn reload`: Reload configuration
- `fn save_service`: Save service configuration to TOML file
- `fn delete_service_config`: Delete service configuration file
- `fn create_service`: Create a new service from configuration
- `fn get_service_config`: Get service configuration
- `fn update_service`: Update service configuration
- `struct SupervisorBuilder`: Supervisor builder for fluent configuration
- `fn new`: -
- `fn services_dir`: -
- `fn runtime_dir`: -
- `fn log_dir`: -
- `fn supervision_interval`: -
- `fn shutdown_timeout`: -
- `fn enable_cgroups`: -
- `fn max_concurrent_ops`: -
- `fn default_restart`: -
- `fn default_health`: -
- `fn build`: -
- `fn to_compose_mapping`: Map this service onto a Compose *service* mapping
- `fn export_to_compose`: Compose a document for this single service. This service has no image, and `ServiceConfig` has no image field.
- `fn export_services_to_compose`: Compose document for a set of service configs

## `crates/pn-next/src/termux_boot/mod.rs`  278 lines

```
//! Termux:Boot Integration Module
//! Provides auto-start capabilities for services at Termux boot
```

Public items - first doc line, `-` when undocumented:

- `struct TermuxBootConfig`: -
- `struct TermuxBootManager`: Termux:Boot manager for service auto-start
- `fn new`: Create a new Termux:Boot manager
- `fn initialize`: Initialize Termux:Boot integration
- `fn add_auto_start_service`: Add a service to auto-start list
- `fn remove_auto_start_service`: Remove a service from auto-start list
- `fn get_auto_start_services`: Get list of auto-start services
- `fn create_service_script`: Generate individual service boot script
- `fn is_available`: Check if Termux:Boot is available
- `fn boot_script_path`: Get boot script path

## `crates/pn-next/src/wake_lock/mod.rs`  435 lines

```
//! Wake Lock Management Module
//! Keeps services alive during Android doze mode and battery optimization
```

Public items - first doc line, `-` when undocumented:

- `enum WakeLockType`: -
- `struct WakeLockConfig`: -
- `struct WakeLockState`: -
- `struct WakeLockManager`: Wake lock manager
- `fn new`: Create a new wake lock manager
- `fn initialize_android`: -
- `fn initialize_android`: -
- `fn acquire`: Acquire a wake lock for a service
- `fn release`: Release a wake lock for a service
- `fn get_state`: Get wake lock state for a service
- `fn get_all_states`: Get all wake lock states
- `fn is_held`: Check if a service has an active wake lock
- `fn start_auto_release_monitor`: Start auto-release monitor
- `fn request_battery_exemption`: Request a battery optimization exemption on Android.
- `fn add_to_battery_whitelist`: Add service to battery whitelist
- `fn set_service_wake_lock`: Set wake lock type for a service
- `fn release_all`: Release all wake locks

## `crates/pn-next/src/watchdog/mod.rs`  736 lines

```
//! Watchdog Integration Module
//! Auto-restart failed containers and services with policy-based recovery
```

Public items - first doc line, `-` when undocumented:

- `struct WatchdogConfig`: -
- `enum RestartPolicy`: -
- `struct EscalationConfig`: -
- `struct EscalationLevel`: -
- `enum EscalationAction`: -
- `enum MaxRestartsAction`: -
- `enum WatchdogEvent`: -
- `struct ServiceWatchdogState`: -
- `struct HealthCheckResult`: -
- `struct WatchdogManager`: Watchdog manager
- `fn new`: Create a new watchdog manager
- `fn with_events`: Set event broadcaster
- `fn with_restart_callback`: Set restart callback
- `fn with_health_check_callback`: Set health check callback
- `fn subscribe_events`: Get event receiver
- `fn register_service`: Register a service for watchdog monitoring
- `fn unregister_service`: Unregister a service
- `fn get_policy`: Get service policy
- `fn set_policy`: Set service policy
- `fn check_service`: Perform health check on a service
- `fn handle_failure`: Handle service failure
- `fn get_state`: Get service state
- `fn get_all_states`: Get all service states
- `fn reset_service`: Reset the service state after manual intervention.
- `fn start_monitoring`: Start watchdog monitoring loop
- `fn shutdown`: Shutdown watchdog

## `crates/pn-next/src/web_dashboard/mod.rs`  829 lines

```
//! Web UI Dashboard Module
//! Real-time monitoring and management dashboard for PDN
```

Public items - first doc line, `-` when undocumented:

- `struct WebDashboardConfig`: -
- `struct DashboardState`: -
- `fn new`: -
- `fn add_event`: -
- `struct ContainerInfo`: -
- `struct ServiceInfo`: -
- `struct SystemStats`: -
- `struct DashboardEvent`: -
- `struct PortInfo`: -
- `struct ContainerActionRequest`: -
- `struct ServiceActionRequest`: -
- `struct CreateContainerRequest`: -
- `struct PortMapping`: -
- `struct VolumeMount`: -
- `struct CreateServiceRequest`: -
- `struct HealthCheckConfig`: -
- `struct WebDashboard`: Web dashboard server
- `fn new`: -
- `fn start`: -
- `fn get_state`: -

## `crates/pn-next/src/workflow.rs`  274 lines

```
//! Workflow engine for PN Next
```

Public items - first doc line, `-` when undocumented:

- `struct WorkflowConfig`: -
- `struct RetryPolicy`: -
- `struct WorkflowEngine`: Workflow engine
- `fn new`: -
- `fn start`: -
- `fn stop`: -
- `fn register_workflow`: -
- `fn execute_workflow`: -
- `fn get_execution`: -
- `fn cancel_execution`: -
- `fn list_workflows`: -
- `struct Workflow`: -
- `struct WorkflowStep`: -
- `enum StepAction`: -
- `struct VolumeMount`: -
- `struct ResourceLimits`: -
- `enum FailureAction`: -
- `enum SuccessAction`: -
- `struct WorkflowInput`: -
- `struct WorkflowOutput`: -
- `struct WorkflowExecution`: -
- `enum WorkflowStatus`: -

## `crates/pn-next/tests/cgroup_honesty.rs`  154 lines

```
//! WORKLIST A20: `setup_cgroup` must not report a resource limit it never bound.
//!
//! Before the fix, `start_service` composed `<runtime_dir>/cgroups/<name>`,
//! `create_dir_all`ed it, and wrote `cgroup.procs`, `cpu.max` and `memory.max`
//! into that plain directory. Those are ordinary files: the writes succeeded,
//! `setup_cgroup` returned `Ok(())`, and no kernel limit existed.
//!
//! The fix has two halves, and this test pins both. A limit this host cannot
//! bind is (a) never faked - no plain-file cgroup is written - and (b) never a
//! reason to refuse the service, because most non-root hosts and the CI runner
//! have no writable cgroup v2 mount; refusing would trade a false claim for a
//! broken feature. The first version of this fix refused, and CI caught it:
//! `daemon_integration_test` failed 4/4 with
//! `cgroup limits UNAVAILABLE for service 'restart-test'`.
```

## `crates/pn-next/tests/cgroup_throttle_honesty.rs`  57 lines

```
//! Base-tree negative control for the supervisor's cgroup honesty (WORKLIST A20, adoption
//! S8 of `research/DISSECTION_SHROOT.md`).
//!
//! This file deliberately uses only the public surface that exists on the base tree
//! (0c8801d), so the same test can be run against it. On the base tree the quota manager's
//! throttle wrote `memory.max` into `<cgroups_mount>/pdn/<service>` as an ordinary file and
//! returned `Ok()` - a caller saw a completed write and a limit that did not exist - so the
//! absence assertion below fails there. With the private cgroupfs view the write is refused
//! by name and no file claims the limit.
```

## `crates/pn-next/tests/cgroup_view.rs`  336 lines

```
//! Adoption S8 (`research/DISSECTION_SHROOT.md`): a private cgroupfs *view*.
//!
//! The reference is `native/shroot/src/cgroup.c` at commit `7ab99d6`, which serves a private
//! file enum and refuses with an explicit errno (`-EBUSY`, `-EPERM`, `-EACCES`, `-EROFS`)
//! what it cannot honour rather than pretending. The two directions are pinned here:
//!
//! (a) a knob PDN can honour from userspace - membership, the sampled CPU/RSS figures,
//!     answered from `/proc` - is served, and the assertions tie the value to the platform
//!     (a live pid is listed and a pid `/proc` does not show is not; the sampled RSS equals
//!     the platform's own `VmRSS` for a process other than this one; the sampled CPU counter
//!     moves when the process burns CPU) so a constant could not satisfy them;
//! (b) a knob PDN cannot honour - a memory, cpuset or device limit - is refused with a named
//!     errno, and no file anywhere claims the limit is in force. That second half is the
//!     assertion that would have caught the original dishonesty (WORKLIST A20).
```

## `crates/pn-next/tests/readiness_handshake.rs`  169 lines

```
//! Start coordination acceptance tests for WORKLIST C10: readiness is one byte
//! on a pipe (runc's `fifoFile.Write([]byte("0"))`), and a start that never gets
//! that byte is a NAMED failure rather than a silent success.
//!
//! The service config is built by parsing TOML rather than by a struct literal,
//! so this exact test source also runs against the base tree, where
//! `readiness_timeout_ms` is an unknown key: there the supervisor spawns the
//! child and returns immediately, which is what the base control quotes.
//!
//! Ordering is asserted, not eventual success: the signalling child writes its
//! byte after `SIGNAL_DELAY_MS` and then lives for 30 s, so a start that returns
//! only after the delay *and* while the child is still alive can only have been
//! released by the byte. Every test bounds its own wait and kills its own child.
```

## `crates/pn-next/tests/restart_policy.rs`  285 lines

```
//! B14 - the restart decision, asserted over the policy alone.
//!
//! No supervisor, no subprocess, no clock, no sleep: every row below is plain data
//! in and one verdict out, so an exit code, a manual stop or a backoff duration is
//! checked by equality rather than by watching a process. The table is printed with
//! `-- --nocapture` so the reader can see exactly which inputs map to which verdict.
```

## `crates/pn-next/tests/runtime_cgroup_honesty.rs`  168 lines

```
//! WORKLIST A27: `Runtime::apply_resource_limits` must not write a cgroup limit file that
//! enforces nothing.
//!
//! Before the fix this path `create_dir_all`ed `rootfs/sys/fs/cgroup/container_<id>` and wrote
//! `cpu.max`, `memory.max`, `memory.swap.max`, `pids.max` and `cpu.weight` into it with
//! `std::fs::write`. Those are ordinary files in a directory that is not a cgroup mount: the
//! writes succeeded and a caller was left to assume a limit was in force when nothing enforced
//! it. That is the last site of the class WORKLIST A20 fixed in the supervisor and S8 fixed in
//! the throttle/quotas path. The fix routes every knob through the private cgroupfs view
//! (`pn_next::CgroupView`), which refuses a resource policy it cannot enforce by name.
//!
//! The test is the same shape as `no_file_claims_a_limit_the_supervisor_could_not_bind`
//! (`tests/cgroup_view.rs`) and the A20 control (`tests/cgroup_honesty.rs`): it starts the
//! runtime path with limits set, walks the runtime tree, and fails if ANY cgroup control file
//! exists as an ordinary file. It pins the other half too - the caller is TOLD, by a named
//! warning, which limit was not applied - so a silent drop cannot pass.
```

## `crates/pn-next/tests/supervisor_e2e.rs`  821 lines

```
//! Service Supervisor E2E Integration Tests
//!
//! Tests for service supervisor lifecycle, persistence, restart policies,
//! and circuit breaker functionality.
//!
//! Restart-policy tests observe an actual state transition instead of sleeping
//! a fixed offset and hoping the supervisor got there in time. A fixed
//! `sleep(3s); assert_eq!(state, expected)` is a race: it samples the supervisor
//! at an arbitrary instant and passes or fails by scheduling. Waiting for the
//! transition, with a deadline an order of magnitude larger than any delay used
//! here, turns that race into a slow test at worst — never a red one.
```

# crates/podnext


## `crates/podnext/examples/test_daemon_client.rs`  47 lines

*No `//!` module doc. The map records this gap.*

## `crates/podnext/src/cli/backup.rs`  51 lines

```
//! `podnext backup` - container snapshot management.
```

Public items - first doc line, `-` when undocumented:

- `enum BackupCommands`: -

## `crates/podnext/src/cli/build.rs`  76 lines

```
//! `podnext build` - the local image builder and the build-service sub-commands.
```

Public items - first doc line, `-` when undocumented:

- `enum BuildCommands`: -

## `crates/podnext/src/cli/common.rs`  78 lines

```
//! Shared value enums for the CLI surface.
//!
//! The output format plus the option families more than one sub-command accepts
//! (`--restart-policy`, `--resource`, `--min-level`, the wake-lock type, the
//! backup type, the completion shell). They live together because no single
//! domain module owns them.
```

Public items - first doc line, `-` when undocumented:

- `enum BackupTypeArg`: -
- `enum OutputFormat`: -
- `enum RestartPolicyArg`: -
- `enum ResourceTypeArg`: -
- `enum LogLevelArg`: -
- `enum WakeLockTypeArg`: -
- `enum Shell`: -

## `crates/podnext/src/cli/image.rs`  78 lines

```
//! `podnext image` - the local OCI image cache.
```

Public items - first doc line, `-` when undocumented:

- `enum ImageCommands`: -

## `crates/podnext/src/cli/network.rs`  62 lines

```
//! `podnext network` - isolated bridge networks.
```

Public items - first doc line, `-` when undocumented:

- `enum NetworkCommands`: -

## `crates/podnext/src/cli/service.rs`  219 lines

```
//! Service supervision: `podnext service` and the daemon, watchdog, quota and log
//! command families that share its shape.
```

Public items - first doc line, `-` when undocumented:

- `enum ServiceCommands`: -
- `enum DaemonCommands`: -
- `enum WatchdogCommands`: -
- `enum QuotaCommands`: -
- `enum LogCommands`: -

## `crates/podnext/src/cli/system.rs`  218 lines

```
//! The platform command families: D-Bus and socket IPC, Termux boot and wakelock,
//! the policy engine, gateway configuration, GitOps reconciliation and the
//! dashboard server.
```

Public items - first doc line, `-` when undocumented:

- `enum DbusCommands`: -
- `enum SocketCommands`: -
- `enum BootCommands`: -
- `enum WakelockCommands`: -
- `enum PolicyCommands`: -
- `enum ConfigCommands`: -
- `enum GitopsCommands`: -
- `enum DashboardCommands`: -

## `crates/podnext/src/cli/volume.rs`  57 lines

```
//! `podnext volume` - persistent data volumes.
```

Public items - first doc line, `-` when undocumented:

- `enum VolumeCommands`: -

## `crates/podnext/src/cli.rs`  767 lines

```
//! Clap command surface for the podnext CLI.
//!
//! Holds the two top-level types - `Cli` and the `Commands` enum every
//! sub-command hangs off. The per-domain sub-command enums and the shared value
//! enums live in the sibling modules under `cli/` and are re-exported here, so
//! the rest of the crate keeps naming them as `crate::<Type>`.
//!
//! | module | holds |
//! |---|---|
//! | `common` | `OutputFormat` and the option families several domains accept |
//! | `image` / `network` / `volume` | the container-storage sub-commands |
//! | `service` | service, daemon, watchdog, quota and log |
//! | `system` | D-Bus, socket, boot, wakelock, policy, config, GitOps, dashboard |
//! | `backup` / `build` | snapshot management and the image builder |
```

Public items - first doc line, `-` when undocumented:

- `struct Cli`: -
- `fn parse_or_exit`: Parse the command line, answering a rejected invocation with the page of the command that owns it
- `enum Commands`: -

## `crates/podnext/src/commands/build.rs`  732 lines

```
//! `podnext build` — materialise a Dockerfile into the local image store.
//!
//! Extracted from `commands/mod.rs` by the roadmap 9.2 CLI split. The helpers
//! below (Dockerfile preprocessing, glob expansion, COPY) are used only here.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_build_command`: -
- `fn cmd_build_local`: -
- `fn build_local_image`: Materialise a Dockerfile build into the runtime's image store
- `fn copy_into_rootfs`: Copy a build-context path into the image rootfs. Dockerfile COPY and ADD use this function.
- `fn copy_one`: Copy one already-resolved source path into the rootfs.
- `fn refuse_unsupported_build_features`: Refuse Dockerfile shapes this builder cannot honour
- `fn expand_glob`: Expand a COPY/ADD source pattern against the build context
- `fn glob_match`: `*` matches any run of characters, `?` exactly one, and `[...]` one character from a set or range. Examples are `[abc]`, `[a-z]` and `[!0-9]`.
- `fn matches_from`: -
- `fn parse_class`: Parse a `[...]` class at the start of `pat`. Return its characters, whether the class is negated, and the pattern after the class. Negation uses `[!...]` or `[^...]`.
- `fn copy_dir_all`: -
- `fn preprocess_build_args`: Consume `ARG` instructions from Dockerfile content
- `fn strip_arg_quotes`: -
- `fn substitute_build_args`: Replace `$NAME` / `${NAME}` with the value of a declared build arg.
- `fn no_build_backend`: Every `build` subcommand except `build local` needs a build service that does not exist
- `fn cmd_build_submit`: -
- `fn cmd_build_status`: -
- `fn cmd_build_cancel`: -
- `fn cmd_build_list`: -
- `fn cmd_build_artifacts`: -
- `fn cmd_build_logs`: -

## `crates/podnext/src/commands/compose.rs`  36 lines

```
//! `podnext compose` — export PDN service configuration to a Docker Compose file.
//!
//! Loading reuses `ServiceSupervisor::load_service_from_file`, so a file that is
//! not a valid supervised service (missing name or command) is rejected here
//! before export. The exporter itself is in `pn_next::supervisor`.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_compose_command`: Export the given service definition file(s) as one Compose document.

## `crates/podnext/src/commands/container.rs`  757 lines

```
//! Container commands: install, list, login, run, ps, sessions, attach, docker, kill, remove,
//! info.
//!
//! Split out of `main.rs` for roadmap section 9.2. Re-exported with `use commands::container::*`.
```

Public items - first doc line, `-` when undocumented:

- `fn cmd_install`: -
- `fn default_container_name`: Derive a container name from an image reference
- `fn cmd_list`: -
- `const MAPPED_USER_NOTE`: Printed once by `run`/`login`, on **stderr** so the machine-readable stdout stream is untouched, before the guest takes the terminal (roadmap section 
- `fn cmd_login`: -
- `fn cmd_run`: -
- `fn cmd_ps`: -
- `fn cmd_sessions`: -
- `fn cmd_attach`: `podnext attach <container>` reconnects to a running detached session. See PDN#57.
- `fn cmd_docker`: -
- `fn cmd_kill`: -
- `fn cmd_remove`: -
- `fn cmd_info`: -
- `struct SerializableInfo`: -

## `crates/podnext/src/commands/dbus.rs`  245 lines

```
//! D-Bus broker commands: `podnext dbus serve`, `list`, `call` and `monitor`.
//!
//! Split out of `commands/mod.rs` for roadmap section 9.2. Re-exported with
//! `pub use dbus::*;` in `commands/mod.rs`.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_dbus_command`: -
- `fn cmd_dbus_serve`: Listen on the broker socket until interrupted.
- `fn cmd_dbus_list`: -
- `fn dbus_roundtrip`: Speak PDN's own broker framing: a 4-byte little-endian length prefix followed by one JSON message
- `fn cmd_dbus_call`: -
- `fn cmd_dbus_monitor`: -

## `crates/podnext/src/commands/gitops.rs`  460 lines

```
//! `podnext gitops` and `podnext dashboard` command implementations.
//!
//! Extracted from `commands/mod.rs` by the roadmap 9.2 CLI split.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_gitops_command`: -
- `fn gitops_manager`: Build a manager pointed at the configured repo and load its state
- `fn cmd_gitops_sync`: -
- `fn cmd_gitops_status`: -
- `fn cmd_gitops_resources`: -
- `fn cmd_gitops_drift`: Report what is known about drift. Do not pretend to know more.
- `fn cmd_gitops_reconcile`: -
- `fn execute_dashboard_command`: -
- `fn cmd_dashboard_start`: -
- `fn cmd_dashboard_stop`: -
- `fn cmd_dashboard_status`: -
- `fn dashboard_pid_path`: Where the dashboard records its pid, next to the other runtime state.
- `fn read_pid`: -
- `fn process_alive`: -

## `crates/podnext/src/commands/image.rs`  757 lines

```
//! Image and registry commands: `podnext image …` and `podnext search`.
//!
//! Split out of `main.rs` for roadmap section 9.2 ("no CLI file over ~800 lines"). The dispatch in
//! `main.rs` pulls these back in with `use commands::image::*`, so the command surface — and
//! therefore `--help` — is unchanged, which `scripts/dump_cli_help.py` proves byte for byte.
```

Public items - first doc line, `-` when undocumented:

- `fn cmd_search`: -
- `fn execute_image_command`: -
- `fn cmd_image_pull`: -
- `fn cmd_image_push`: -
- `fn cmd_image_rm`: -
- `fn cmd_image_tag`: -
- `fn cmd_image_history`: -
- `fn read_store_metadata`: Read the `metadata.json` of a locally built image from the container store
- `fn cmd_image_inspect`: -
- `fn du_size_bytes`: Bytes used by `path`: `Some((bytes, is_lower_bound))`, or `None` when the size cannot be measured at all
- `fn walk_size_bytes`: Sum the sizes of the readable regular files under `path`, skipping everything that cannot be read or followed (unreadable directories, dangling symlin
- `fn cmd_image_ls`: -

## `crates/podnext/src/commands/log.rs`  191 lines

```
//! Structured log commands: `podnext log query`, `tail`, `stats` and `flush`.
//!
//! Split out of `commands/mod.rs` for roadmap section 9.2. Re-exported with
//! `pub use log::*;` in `commands/mod.rs`.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_log_command`: -
- `fn cmd_log_query`: -
- `fn cmd_log_tail`: -
- `fn cmd_log_stats`: -
- `fn cmd_log_flush`: -

## `crates/podnext/src/commands/mod.rs`  880 lines

```
//! CLI command implementations for service orchestration
```

Public items - first doc line, `-` when undocumented:

- `fn execute_backup_command`: -

## `crates/podnext/src/commands/network.rs`  247 lines

```
//! Network commands: `podnext network …`.
//!
//! Split out of `main.rs` for roadmap section 9.2. Re-exported with `use commands::network::*`.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_network_command`: -
- `fn cmd_network_create`: -
- `fn cmd_network_ls`: -
- `fn cmd_network_inspect`: -
- `fn cmd_network_rm`: -
- `fn cmd_network_connect`: -
- `fn cmd_network_disconnect`: -

## `crates/podnext/src/commands/policy.rs`  197 lines

```
//! Policy-engine commands: `podnext policy eval`, `list`, `load` and `presets`.
//!
//! Split out of `commands/mod.rs` for roadmap section 9.2. Re-exported with
//! `pub use policy::*;` in `commands/mod.rs`.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_policy_command`: -
- `fn cmd_policy_eval`: Evaluate the configured policy rules against one service
- `fn cmd_policy_list`: -
- `fn cmd_policy_load`: -
- `fn cmd_policy_presets`: -

## `crates/podnext/src/commands/power.rs`  153 lines

```
//! The `boot` and `wakelock` supervision commands, extracted from `commands/mod.rs`.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_boot_command`: -
- `fn cmd_boot_enable`: -
- `fn cmd_boot_disable`: -
- `fn cmd_boot_status`: -
- `fn execute_wakelock_command`: -
- `fn cmd_wakelock_acquire`: -
- `fn cmd_wakelock_release`: -
- `fn cmd_wakelock_status`: -

## `crates/podnext/src/commands/quota.rs`  318 lines

```
//! Resource quota commands: `podnext quota set`, `get`, `usage` and `list`.
//!
//! Split out of `commands/mod.rs` for roadmap section 9.2. Re-exported with
//! `pub use quota::*;` in `commands/mod.rs`.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_quota_command`: -
- `fn cmd_quota_set`: -
- `fn cmd_quota_get`: -
- `fn cmd_quota_usage`: Live usage for one service, next to the limit configured for it
- `fn cmd_quota_list`: -
- `fn parse_quota_limit`: -

## `crates/podnext/src/commands/service.rs`  739 lines

```
//! The `service` and `daemon` supervision commands.
//!
//! Extracted from `commands/mod.rs` by the roadmap 9.2 CLI split. `get_supervisor` and the
//! `output_*` formatters stay in the parent module and are reached through `super::`, because
//! the quota, policy, boot and other domains use them too.
```

Public items - first doc line, `-` when undocumented:

- `fn daemon_socket`: The daemon socket, resolved from the environment rather than hard-coded: a home that moves, for example a second profile, a relocated install, or `TERMUX__HOME`, must ta
- `fn send_daemon_command`: Send a command to the daemon and get response
- `fn is_daemon_running`: Check if the daemon runs.
- `fn execute_service_command`: -
- `fn cmd_service_start`: -
- `fn cmd_service_stop`: -
- `fn cmd_service_restart`: -
- `fn cmd_service_list`: -
- `fn cmd_service_status`: -
- `fn print_services`: Print services from daemon response
- `fn print_service_status`: Print service status from daemon response
- `fn print_services_local`: Print services from local supervisor
- `fn print_service_status_local`: Print service status from local supervisor
- `fn cmd_service_logs`: -
- `fn cmd_service_enable`: -
- `fn cmd_service_disable`: -
- `fn cmd_service_load`: -
- `fn cmd_service_create`: -
- `fn execute_daemon_command`: -
- `fn cmd_daemon_start`: -
- `fn cmd_daemon_stop`: -
- `fn cmd_daemon_status`: -
- `fn cmd_daemon_reload`: -
- `fn cmd_daemon_logs`: -

## `crates/podnext/src/commands/socket.rs`  44 lines

```
//! Socket-activation commands: `podnext socket register`, `unregister` and `list`.
//!
//! Split out of `commands/mod.rs` for roadmap section 9.2. Re-exported with
//! `pub use socket::*;` in `commands/mod.rs`.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_socket_command`: -
- `fn cmd_socket_register`: -
- `fn cmd_socket_unregister`: -
- `fn cmd_socket_list`: -

## `crates/podnext/src/commands/sync.rs`  586 lines

```
//! Container filesystem commands: `podnext copy`, `sync`, `reset`, `rename` and the hole-aware
//! copy machinery they share.
//!
//! Split out of `main.rs` for roadmap section 9.2. Re-exported with `use commands::sync::*`.
```

Public items - first doc line, `-` when undocumented:

- `fn cmd_rename`: Rename a container: the directory name *is* the identity, so the move plus a rewritten `metadata.json` is the whole operation.
- `fn split_spec`: Split `[CONTAINER:]PATH`
- `fn guest_to_host`: Join a guest path onto a rootfs, refusing anything that leaves it.
- `fn cmd_copy`: -
- `fn cmd_reset`: Reset a container: remove it and reinstall the image it came from
- `struct SyncReport`: -
- `fn cmd_sync`: Mirror `source` onto `destination`
- `fn file_digest`: -
- `fn copy_preserving_holes`: Copy `src` onto `dst`, keeping holes as holes
- `fn copy_data_regions`: Copy only the allocated regions, leaving the source's holes as holes.
- `fn lseek`: `lseek(2)` returning an absolute offset, so `SEEK_DATA`/`SEEK_HOLE` can drive the hole-aware copy above.
- `fn same_content`: True when the destination already holds this content.
- `fn sync_one`: Sync a single file, placing it inside `dst` when `dst` is a directory.
- `fn mirror_dir`: Walk `src` and mirror every entry into `dst`
- `fn prune_extras`: Remove destination entries that have no counterpart in the source
- `fn copy_dir_recursive`: A recursive copy that keeps symlinks as symlinks. A copied tree does not silently balloon, unlike a copy that follows every link.

## `crates/podnext/src/commands/volume.rs`  273 lines

```
//! Volume commands: `podnext volume …`.
//!
//! Split out of `main.rs` for roadmap section 9.2. Re-exported with `use commands::volume::*`, so
//! the dispatch and the `--help` surface are unchanged (`scripts/dump_cli_help.py` proves it).
```

Public items - first doc line, `-` when undocumented:

- `fn execute_volume_command`: -
- `fn cmd_volume_create`: -
- `fn cmd_volume_ls`: -
- `fn cmd_volume_inspect`: -
- `fn cmd_volume_rm`: -
- `fn cmd_volume_prune`: -

## `crates/podnext/src/commands/watchdog.rs`  115 lines

```
//! The `watchdog` supervision commands, extracted from `commands/mod.rs`.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_watchdog_command`: -
- `fn cmd_watchdog_status`: -
- `fn cmd_watchdog_set_policy`: -
- `fn cmd_watchdog_reset`: -
- `fn cmd_watchdog_events`: -

## `crates/podnext/src/daemon.rs`  595 lines

```
//! podnext-daemon - Persistent service supervisor daemon
//!
//! Runs as a background daemon managing container services via Unix socket IPC.
```

## `crates/podnext/src/direct.rs`  412 lines

```
//! Direct-execution mode: `podnext -r <rootfs> -- <cmd>`.
//!
//! Two entry points reach the same config. `scan_direct_execution` runs before
//! clap, because a `--`-separated command must not be re-parsed as flags, and
//! refuses the flags the engine cannot honour; `execute_direct` is the
//! clap-parsed path. Both build the same `proot_core::ProotConfig`.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_direct`: -
- `fn execute_direct_manual`: -
- `fn scan_direct_execution`: The manual `podnext -r <rootfs> -- <cmd>` path, taken before clap.

## `crates/podnext/src/dispatch.rs`  125 lines

```
//! Sub-command dispatch: one `match` from `Commands` to the command
//! implementation that runs it.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_command`: -

## `crates/podnext/src/doctor.rs`  789 lines

```
//! `podnext doctor` - a host and guest diagnostic that reports what it measured.
//!
//! Every check runs and reports one of three verdicts, and each carries the value or the
//! command it came from. Nothing here is inferred from the absence of an error: a check
//! that cannot run says so (`Unknown`) instead of passing.
```

Public items - first doc line, `-` when undocumented:

- `enum Verdict`: -
- `struct Check`: -
- `fn collect`: Run every host check
- `fn render_text`: -
- `fn worst_verdict`: -
- `fn capability_line`: The capability probes rendered as one line, for callers that must name WHY a capability is absent rather than assert that it is

## `crates/podnext/src/main.rs`  479 lines

```
//! podnext CLI - Advanced container and service management for Termux/Android
//!
//! A comprehensive CLI that works with proot-distro-core, pn-next, and pn-gateway
//! to provide full container lifecycle management, service orchestration, and monitoring.
```

Public items - first doc line, `-` when undocumented:

- `fn output_table`: -
- `fn output_json`: -
- `fn output_yaml`: -

## `crates/podnext/src/native/digest.rs`  41 lines

```
//! Digest validation before path composition (WORKLIST B1).
//!
//! Comparator: proot-distro `docker/cache.py:validate_digest`, described in NEXT_DEV.md §5.1 as
//! "a choke point before any conversion of an untrusted digest into a filesystem path"; their
//! `_oci_blob_path` validates first so "a crafted index.json cannot route the lookup through a
//! member name".
//!
//! Nothing here does I/O. `image.rs` validates at the one place it composes a request path out of
//! a digest, before the URL exists; the digest also reaches the digest-mismatch check downstream,
//! where a validated value is the only kind that means what it says.
```

Public items - first doc line, `-` when undocumented:

- `fn validate_digest`: Validate a content digest *before* anything composes a path, URL or slice from it (B1)

## `crates/podnext/src/native/image.rs`  984 lines

```
//! The OCI / Docker Hub install path: registry search, manifest and layer
//! fetch, and [`install_image_native`].
//!
//! Split out of `native` in wave 7; behaviour unchanged.
```

Public items - first doc line, `-` when undocumented:

- `struct DockerHubSearchResponse`: -
- `struct DockerHubImage`: -
- `fn search_docker_hub`: -
- `struct DockerHubTokenResponse`: -
- `struct ManifestList`: -
- `struct ManifestDescriptor`: -
- `struct Platform`: -
- `fn smoke_test_rootfs`: -
- `fn reap_stale_staging`: Reclaim staging residue: every `.tmp-install-*` / `.tmp-replaced-*` directory that no live install owns. See A3b.
- `fn install_image_native`: -
- `fn install_archive_native`: Install from a local archive, for example `./rootfs.tar.gz` or `./image.oci.tar`, or from an http or https URL to one. Verify the archive against a sha256 checksum when one is given.
- `fn verify_image_available`: Check that an image reference can actually be fetched, before any destructive action.

## `crates/podnext/src/native/meta.rs`  299 lines

```
//! Container and session listing, the image metadata re-apply, and the
//! command-line mount-spec helpers.
//!
//! Split out of `native` in wave 7; behaviour unchanged.
```

Public items - first doc line, `-` when undocumented:

- `fn list_containers_native`: -
- `fn parse_mount_spec`: Parse a mount spec from the command line: `HOST:GUEST[:ro|rw]`
- `fn parse_mount_specs`: Parse every `--bind`/`--volume` spec, reporting the first bad one.
- `fn apply_image_runtime_config`: Reinstate the environment and working directory an image was built with
- `struct SessionRow`: -
- `fn session_registry`: Every live session, in one deterministic order
- `fn list_sessions_native`: -
- `fn list_sessions_native_full`: The caller that used the full in-memory list now reads the same registry: two sources is exactly how the renderings diverged (A2).
- `fn kill_container_native`: -
- `fn remove_container_native`: -

## `crates/podnext/src/native/retry.rs`  141 lines

```
//! The transfer policy for the registry client (WORKLIST B2 and B3).
//!
//! Two decisions, each a pure function over plain data so it can be tested without a socket:
//!
//! - **Where a redirect may take the request, and whether the credentials travel with it** (B2;
//!   comparators `docker/transport.py:same_auth_origin` and `download.py:check_redirect`).
//! - **When a failed transfer may be repeated, how long to wait, and whether the body arrived
//!   whole** (B3; comparator `helpers/download.py`).
//!
//! Every socket stays in `image.rs`, which is the only caller. `now` is a parameter rather than a
//! call to the clock, so the date form of `Retry-After` is testable too.
```

Public items - first doc line, `-` when undocumented:

- `const MAX_ATTEMPTS`: Attempts allowed for one layer, the first included.
- `const MAX_DELAY`: Ceiling on a single wait, whatever `Retry-After` asks for.
- `enum Decision`: -
- `fn next_attempt`: Decide what follows a failed attempt (B3)
- `fn retry_after_delay`: The `Retry-After` header as a wait
- `fn ensure_complete_body`: Verify that the body is as long as the server stated. See B3.
- `enum Redirect`: -
- `fn plan_redirect`: Decide where a redirect can go, and whether the credentials travel with it. See B2.

## `crates/podnext/src/native/session.rs`  747 lines

```
//! The container session path: foreground login and run, signal forwarding to
//! the guest, and the detached-session PTY supervisor.
//!
//! Split out of `native` in wave 7; behaviour unchanged.
```

Public items - first doc line, `-` when undocumented:

- `fn login_container_native`: -
- `fn direct_child_pid`: The PID of this process's only direct child during a foreground run. That child is the proot process the runtime spawned.
- `fn forward_to_guest`: Forward `sig` to the running guest
- `fn run_container_native`: -
- `fn is_session_host`: True when this process is the detached supervisor that `run --detach` re-invokes through `attach`, rather than an attaching client.
- `fn spawn_detached_session`: Start `container` in the background under a supervised PTY. Use `run --detach`.
- `fn attach_session`: Reconnect to the running detached session for `container`. Use `podnext attach`.
- `fn run_session_host`: The detached supervisor: owns the PTY for the guest and serves `attach`

## `crates/podnext/src/native.rs`  263 lines

```
//! Native implementations for podnext
//!
//! This module provides native implementations of all podnext commands
//! using the Rust crates directly instead of wrapping proot-distro.
//!
//! Split by subsystem: [`image`] holds the OCI/registry install path, [`meta`]
//! the container/session listing and mount helpers, [`session`] the run/login
//! path and the detached-session supervisor.
```

## `crates/podnext/src/subcommands.rs`  208 lines

```
//! The commands that print rather than orchestrate: host diagnostics, the
//! version table, shell completions, the gateway-configuration check and the
//! top-level help screen.
```

Public items - first doc line, `-` when undocumented:

- `fn execute_config_command`: `podnext config validate`: the load site for the gateway configuration
- `fn cmd_doctor`: `podnext doctor`: report what the host and the guest actually look like
- `fn cmd_version`: -
- `fn cmd_completion`: -
- `fn print_help`: -
- `struct SystemInfo`: -
- `struct VersionInfo`: -

## `crates/podnext/src/utils.rs`  69 lines

```
//! Utility functions
```

Public items - first doc line, `-` when undocumented:

- `fn parse_container_path`: -
- `fn dir_size`: -
- `fn format_size`: Format size in human-readable format
- `fn parse_size`: -

## `crates/podnext/tests/argv_nonutf8.rs`  58 lines

```
//! Regression test for Issue #41: non-UTF-8 argv should not panic.
//!
//! Before fix: `std::env::args()` panicked on invalid UTF-8 bytes (e.g. 0xff),
//! causing SIGABRT (exit 134).
//!
//! After fix: `std::env::args_os()` + `to_string_lossy()` converts invalid
//! sequences to replacement characters (�), allowing graceful error handling.
```

## `crates/podnext/tests/automation_contract.rs`  142 lines

*No `//!` module doc. The map records this gap.*

## `crates/podnext/tests/build_copy_escape.rs`  149 lines

```
//! WORKLIST A11 (second half): a COPY whose destination escapes the image root.
//!
//! `podnext build` joins a COPY destination straight onto the image rootfs
//! (`rootfs.join(dest)`), so a `..` component climbs out of it: `COPY x
//! ../../../../escaped.txt` wrote the file outside the image and into the store.
//! This drives the real CLI - the Dockerfile goes through the real parser and the
//! real COPY execution path - and then looks for the byte that landed outside the
//! root.
//!
//! The store is a scratch directory (`HOME` and `PN_TEST_ROOT` both point at it),
//! so the operator's real `~/.podnext` is never touched.
```

## `crates/podnext/tests/cli_reference_contract.rs`  328 lines

```
//! Roadmap section 9.1 contract test: every command and every documented option in
//! `docs/src/cli-reference.md` must exist in the real CLI, and the documentation must not
//! describe exit codes or environment variables that no code path implements.
//!
//! This test exists because the reverse happened: a conflict resolution re-added a phantom
//! exit-code table (3–12), eight inert `PN_*` variables and a `--short`/`--json` option table
//! for `podnext version`, and nothing failed. It runs the built binary, so it measures the
//! product rather than restating the source.
```

## `crates/podnext/tests/daemon_honesty.rs`  220 lines

```
//! WORKLIST A10: `daemon status` must not claim a running daemon when there is none,
//! and `service create` must not exit 0 while persisting nothing.
//!
//! WORKLIST A14 (extended here): `daemon stop`, `daemon reload` and `daemon logs`
//! had the same defect one layer down — all three printed a success line and exited 0
//! with nothing listening. They now share `daemon status`'s socket probe and refuse.
//!
//! Both commands are run as the real binary with `HOME`, `XDG_RUNTIME_DIR`,
//! `XDG_CONFIG_HOME` and every `PN_NEXT_*` path redirected into a throwaway directory
//! under the process temp dir. No fixed path, no container name, and no assumption
//! about whether a daemon happens to run on the host: the socket these commands would
//! answer on lives inside the sandbox, so nothing can be listening there.
```

## `crates/podnext/tests/daemon_ipc.rs`  203 lines

```
//! A18(b): with a daemon actually up, `daemon reload`, `daemon logs` and `daemon stop`
//! report what the daemon answered.
//!
//! The base printed a success line for all three with nothing listening on the socket
//! and no request sent (see `daemon_honesty.rs` for the no-daemon half). Here the daemon
//! is a real [`pn_next::DaemonRuntime`] on its own thread and tokio runtime, bound to the
//! exact socket path the CLI resolves (`$XDG_RUNTIME_DIR/pn-next/daemon.sock`), so the
//! test drives the real IPC surface through the real `podnext` binary.
//!
//! The daemon lives inside this process, so the socket is not an external dependency —
//! it is the fixture under test. If it ever fails to bind, the test names a
//! `FIXTURE-UNAVAILABLE` line and returns rather than asserting on a daemon that is not
//! there.
```

## `crates/podnext/tests/doctor_capabilities.rs`  185 lines

```
//! S5/C7 - the compatibility probe, and the one property that makes it worth having.
//!
//! The probe exists because PDN made claims about the executor that nothing measured: the Docker
//! refusal said "Docker requires namespaces (mount, pid, net) and cgroups v2, which PRoot cannot
//! provide" as prose, and the cgroup path wrote limits into a plain directory and returned Ok().
//!
//! A check that reports `ok` for something it did not measure is the same defect one layer up, so
//! these tests assert the honest shape rather than the happy path:
//!   1. every probe answers with ONE of the four named verdicts - never an empty string, never a
//!      verdict with no detail to explain it;
//!   2. a knob this platform cannot honour is NOT reported `ok` (the cgroupfs case on Android,
//!      where /sys/fs/cgroup is a cgroup2 mount the app cannot write);
//!   3. `subreaper` is `unknown`: PR_SET_CHILD_SUBREAPER has no read-only form, so claiming `ok`
//!      would assert a capability the probe never measured;
//!   4. the Docker refusal carries the measurements, so a user reads why rather than being told;
//!   5. the probe changes nothing it measures: the platform facts it reads are the same before and
//!      after, which is what makes it safe to run before any guest starts.
```

## `crates/podnext/tests/doctor_contract.rs`  129 lines

```
//! Roadmap section 9.3: `podnext doctor` reports host and guest diagnostics, and a failing check
//! is the only thing that changes the exit status. These tests drive the real binary, which is
//! also what lets the command carry the `[stable]` label in the reference.
//!
//! The verdict of a check depends on the host (a CI runner has no proot binary, this device
//! does), so the assertions are about the relationship between the verdicts printed and the
//! exit status, never about a fixed status for a host-dependent check.
```

## `crates/podnext/tests/exit_codes.rs`  229 lines

```
//! The exit-code contract (roadmap section 9.1).
//!
//! Before this file, the exit status was asserted only where a particular test happened to need
//! it. The criterion asks for the taxonomy itself: a caller must be able to tell "you typed it
//! wrong" from "this is documented as unavailable" from "the container does not exist" from
//! "the child exited non-zero", without reading stderr.
//!
//! Measured, not assumed (2026-11-05, target/debug/podnext on device):
//!
//! | class                        | code | stdout | stderr                     |
//! |------------------------------|------|--------|----------------------------|
//! | success                      | 0    | answer | empty                      |
//! | usage error (clap)           | 2    | empty  | `error: unrecognized ...`  |
//! | documented refusal           | 1    | empty  | `Error: ... is not implemented` |
//! | domain error (no such thing) | 1    | empty  | `Error: Container ... not found` |
//! | guest exit code              | N    | as-is  | as-is                      |
//!
//! The last row is the one that cannot be faked: it needs a container that can actually run a
//! command, so it skips with a printed reason when no such fixture exists on this host - and it
//! must skip, not pass, because a test that cannot run is not evidence.
```

## `crates/podnext/tests/guest_identity_probe.rs`  49 lines

```
//! Base-tree negative control for S2 (adoption of `shroot`'s identity/credential syscall table).
//!
//! This file deliberately uses only the surface that exists on the base tree (f2f7a5f): it spawns
//! the real `podnext doctor` binary and parses the JSON, exactly as `doctor_capabilities.rs` does,
//! so the same file compiles and runs there. On the base tree `doctor` reports no row naming the
//! guest's identity: the identity syscalls are implicit in the runtime's `--change-id=0:0:0` argv
//! and named nowhere a caller can read, so the row lookup below fails there. With the table named
//! in `proot_core::identity` and reported by the probe, it passes.
```

## `crates/podnext/tests/image_config_env.rs`  608 lines

```
//! The image's own config, applied at `login` (WORKLIST B5, issue #76), and the
//! kill ladder (WORKLIST B8).
//!
//! **What was measured before this file existed.** `podnext build local` records
//! `env`/`working_dir`/`cmd` in the image's `metadata.json` (#29), and
//! `podnext run` reinstates them; `podnext login` on the same image did not, so
//! the guest logged into an image built with `ENV GREETING=hi` saw `GREETING`
//! unset while `run` saw `hi`. In the other direction `podnext kill` sent
//! `SIGQUIT` and then escalated straight to `SIGKILL`, taking neither the
//! reference's first rung (`SIGTERM`, `commands/proot-distro/kill.py:234-236`)
//! nor a signal the caller named.
//!
//! **How each half is evidenced here, without a network and without depending on
//! a healthy guest.**
//!
//! * Persistence is driven through the real binary: an image is built with
//!   `FROM scratch` (no registry pull) carrying `ENV`, `WORKDIR` and `CMD`, and
//!   the `metadata.json` the build wrote is asserted field by field.
//! * Application at `login` needs a guest that can report its own environment,
//!   so the fixture pattern is used: a candidate container is probed with
//!   `run <c> -- /bin/true`, the first that works is named on stderr, and its
//!   rootfs is copied into a temp store (never linked, never written) carrying
//!   the image config under test. When no candidate can run a trivial command,
//!   one loud `FIXTURE-UNAVAILABLE:` line says what was probed and what came
//!   back, and the test returns: an unrunnable test is not a pass.
//! * The ladder is exercised on real processes: the session registry is pointed
//!   at a temp root with `PN_TEST_ROOT`, the record is written exactly as the
//!   runtime writes it, and the process behind it is a child this test started -
//!   one that ignores the signals it must ignore, so no rung can be passed by
//!   accident.
//!
//! Nothing here reads or writes `~/.podnext`.
```

## `crates/podnext/tests/inspect_missing.rs`  64 lines

```
//! Names that do not resolve must fail the command, not report success with empty output.
//!
//! Measured before this fix (WORKLIST A8): `podnext volume inspect <missing>` and
//! `podnext network inspect <missing>` exited 0 with `[]` on stdout, while `podnext image inspect`
//! correctly exited non-zero. This asserts ONE convention across all three: a non-zero exit, no
//! empty JSON array presented as a result, and the missing name reported on stderr.
//!
//! The store is redirected to a temporary EMPTY directory for the duration of the test. PDN resolves
//! its store from TERMUX__HOME/PREFIX in preference to HOME (measured during the extreme test, when a
//! sibling's HOME override did not move the store), so both are set here. Nothing writes to the real
//! store: these commands only read.
```

## `crates/podnext/tests/install_interrupt.rs`  865 lines

```
//! Install/remove lifecycle (WORKLIST A3b, A4b, B12).
//!
//! Five measured properties of the install path:
//!
//! (a) staging residue that no process owns is reclaimed by the next install of
//!     the same name, which then commits a whole container;
//! (b) `remove --force` clears staging residue too, not only the committed
//!     container - including for a name that was never installed;
//! (c) an install that was interrupted leaves no orphan `podnext` process, no
//!     torn `metadata.json` and no residue the next install cannot clear; the
//!     census is by program identity (`ps -eo comm=`), never a command-line grep;
//! (d) a concurrent same-name install can never destroy a committed container:
//!     exactly one install wins and the survivor is a whole rootfs;
//! (e) a staging tree whose sidecar lock another process holds is live and
//!     survives a reaper running in a third process; once the lock is released the
//!     same tree is residue and is reaped.
//!
//! Residue is PLANTED, never raced. The old fixture slept a fixed `KILL_AFTER`,
//! killed the install and then asserted that the kill had left a staging
//! directory to reclaim. That window is narrow in BOTH directions: measured on
//! this device, a kill 3 s in leaves ZERO residue (the staging tree does not exist
//! yet - the first seconds go to the manifest), and on a CI runner the whole
//! install finishes before the kill (runs 42187850363 and ac62ada: `the killed
//! --force reinstall must leave a staging residue to reclaim: []` with
//! `status=ExitStatus(unix_wait_status(0))`, then `this host completed it faster
//! than the fixture's window`).
//!
//! So the tests now assert the reaper's own contract instead of a race
//! (`crates/podnext/src/native/image.rs:170-256`): a `.tmp-install-*` directory is
//! residue when its sidecar `<staging>.lock` is absent or can be `flock`ed by the
//! observer, and live when another process holds that lock. The fixture creates
//! that state directly, so every assertion is the same on every host.
//!
//! The one remaining SIGKILL test (c) is driven by an observed event - the staging
//! tree appearing - never a clock, PRINTS which branch it took, and asserts only
//! the post-condition that holds whether or not the kill landed mid-flight. No
//! test requires the kill to have produced residue.
//!
//! Every test runs against a private store (`HOME`/`PN_TEST_ROOT` point at a
//! scratch directory), so the operator's real `~/.podnext` is never touched; that
```

## `crates/podnext/tests/install_lock.rs`  176 lines

```
//! WORKLIST A4, end to end: two concurrent installs of ONE name cannot both run.
//!
//! Measured before this (POC-EXTREME-TESTING.md F9, WORKLIST A4): both processes printed
//! `Container '<name>' installed successfully!` and `~/.podnext/locks/` stayed EMPTY through every
//! race, because the lock primitive existed in `proot_distro_core::locking` and nothing on the install
//! path ever took it. This asserts the CLI contract: exactly one install succeeds, the other is refused
//! BEFORE doing any work, and the refusal names the holder's PID - which is the difference between a
//! dead end and a diagnosis.
//!
//! The store lives in a scratch directory (`HOME` and `PN_TEST_ROOT` both point at it), so the
//! operator's real `~/.podnext` is untouched. It asserts its own sandbox condition at the end.
```

## `crates/podnext/tests/install_sources.rs`  511 lines

```
//! Install sources beyond a registry reference (roadmap section 10.1): a local
//! rootfs tarball, a local OCI image-layout archive, and a URL - each with the
//! optional `--checksum` the reference does not have for installs.
//!
//! **What is measured, and against what.** Every test runs the real binary
//! against a private store (`HOME` and `PN_TEST_ROOT` both point at a temp
//! root), so the operator's `~/.podnext` is never touched. The fixtures are
//! built here, offline: a small rootfs tar (optionally gzipped), and an OCI
//! image layout tar assembled from the pieces the format is made of (an
//! `oci-layout` marker, an `index.json`, and `blobs/sha256/*`). The URL test
//! serves its fixture from a loopback listener created for the test, never the
//! network - this device's registry link is intermittent, and a test that needed
//! it would flake.
//!
//! **The reference's rules, asserted where they are observable.** A local
//! source is one with explicit path syntax (`./x`, `/x`, `../x`, `~/x`); a bare
//! token stays a registry reference. The container name is derived from the
//! archive filename (`_derive_local_name`), and every refusal names its reason -
//! no silent success.
//!
//! The whole file is `expected to fail against the base tree`, where `install`
//! takes a registry reference only: the source changes are what make it pass.
```

## `crates/podnext/tests/orphan_reaping.rs`  262 lines

```
//! A22: PDN is a real subreaper, so the descendants of a dead guest are adopted
//! and reaped instead of surviving the session as orphans.
//!
//! Measured defect (campaign logs, second dissection round): PDN never called
//! `prctl(PR_SET_CHILD_SUBREAPER)`, so `grep -rn PR_SET_CHILD_SUBREAPER crates/`
//! returned nothing; when a session died its tracees were reparented to PID 1
//! and outlived it - two orphans of a finished session were alive for 2.730 s
//! and 3.046 s and exited only on SIGTERM. proot-distro reaps its orphans; PDN
//! did not.
//!
//! The condition is *planted*, not raced for: the guest prints its grandchild's
//! pid, the test reads that pid, confirms the grandchild is running, and kills
//! the guest's tracer. The grandchild is then either reaped by podnext (fixed)
//! or reparented and left running (the defect above).
//!
//! This drives the real binary. It needs a container to run a guest in; where
//! none is installed it prints `FIXTURE-UNAVAILABLE:` and returns, because an
//! unrunnable test is not a pass.
```

## `crates/podnext/tests/pull_hygiene.rs`  234 lines

```
//! Pull hygiene: a digest is validated before it becomes a path, the credentials stay bound to
//! their origin, and a failed transfer is repeated only when a repeat can help (WORKLIST B1-B3).
//!
//! All three are pure functions over plain data in `src/native/digest.rs` and `src/native/retry.rs`;
//! every socket stays in `src/native/image.rs`. Nothing in this file opens one: this device's link
//! to Docker Hub is intermittent (a 30 MB pull failed with `Error: error decoding response body`
//! while a 3 MB pull succeeded), so no gate may depend on a transfer.
//!
//! The modules are compiled from their real source rather than re-declared, so these exercise the
//! shipped code and not a copy of it.
```

## `crates/podnext/tests/quoting_table.rs`  165 lines

```
//! The argv quoting table (roadmap section 9.3).
//!
//! What the caller writes must arrive at the guest as the caller meant. The interesting cases are
//! the ones where a shell is usually in the way: `$HOME` must not be expanded, `$(id)` must not be
//! run, `*` must not glob, a quote must stay a quote, and a newline must survive inside a single
//! argument.
//!
//! A measurement note worth keeping, because it cost a wrong conclusion: when this was first
//! probed from a shell, `$(id)` arrived at the guest already expanded - by the *host shell* in the
//! probe, not by podnext. Any test of this property that goes through a shell proves nothing about
//! the tool. Here every argument is handed over as a literal `OsStr` by `Command::arg`, with no
//! shell anywhere in the path, and the guest reports what it received NUL-separated so an argument
//! containing a newline cannot be confused with two arguments.
//!
//! Skips with a printed reason when no container that can run a command is installed: an unrunnable
//! check is not a pass, and a name in `podnext list` is not a container that works.
```

## `crates/podnext/tests/refcount_eviction.rs`  212 lines

```
//! C6 acceptance: reference-counted eviction, proven against the real binary.
//!
//! Two halves, each able to fail on its own:
//!
//! (1) a store unit that is still referenced is **never** deleted, and the
//!     refusal names the count and the holder;
//! (2) the same unit, once its references reach zero, **is** deleted.
//!
//! The store unit is the staging tree an interrupted install leaves behind — the
//! `.tmp-install-*` directory the campaign measured as a permanent leak. Its
//! reference is the live install that holds the sidecar `<tree>.lock`; that is a
//! real reference today, not a planted counter.
//!
//! The reference is planted deterministically (this process takes the `flock`),
//! never raced for: `flock` excludes across processes, so the reaper under test
//! is the real `podnext remove` in a separate process — the production
//! arrangement.
//!
//! Every store path is redirected (`HOME` and `PN_TEST_ROOT`), so no test can
//! write to the operator's real `~/.podnext`.
```

## `crates/podnext/tests/relocated_paths.rs`  93 lines

```
//! Roadmap section 9.2: the binary must honour `TERMUX__HOME`, `TERMUX__PREFIX` and the
//! `XDG_*` variables instead of hard-coding one machine's Termux paths. This drives the real
//! binary with a relocated home and prefix and checks that its defaults follow them.
```

## `crates/podnext/tests/remove_atomic.rs`  290 lines

```
//! WORKLIST A5: `remove` must not half-apply while a session is live.
//!
//! Measured on the base commit (three independent agents): `remove <c> --force`
//! with a live session printed
//!
//! ```text
//! Error: removed '<c>' from the index but failed to delete <dir>: Permission
//! denied (os error 13)
//! ```
//!
//! and exited 1, leaving the guest alive and the directory on disk - while the
//! index had already dropped the record. `rm -rf` then succeeded. The two halves
//! of the operation disagreed.
//!
//! The semantics this file pins down (A5): `remove` REFUSES BEFORE it touches the
//! index when a live session still names the container, naming that session; and
//! otherwise deletes the directory before it drops the record - the index and the
//! directory move together or not at all. A command that cannot remove must change
//! nothing; a command that succeeds must leave neither half behind.
//!
//! A real detached session is racy on a fast host and needs an installed guest, so
//! liveness is planted the way the registry itself defines it: a `{pid}.json`
//! record in `RUNTIME_DIR/sessions` whose pid is a live, non-zombie process with a
//! matching kernel start time, and whose `LOCK_EX` this test holds for its whole
//! lifetime. That is exactly the state `active_sessions()` reads as "a live
//! session" (see `crates/proot-distro-core/src/session.rs`), so it cannot race -
//! the session is live until this test says otherwise.
//!
//! The directory cannot be deleted, the way a live guest makes it: the store's
//! parent directory is made read-only, so `remove_dir_all` unlinks the container's
//! contents and then fails on the final `rmdir` with EACCES - the same error the
//! live session produced. That is order-independent and identical on every run,
//! which is what makes the base half-apply deterministic: the container's marker
//! file goes first, so the store stops recognising the directory although the
//! directory is still there.
```

## `crates/podnext/tests/session_reattach_parity.rs`  279 lines

```
//! C9 (v0.3.0) - parity with containerd's shim, proven rather than asserted.
//!
//! R3 re-worded C9 from "build a shim" to a parity check, on the evidence that the shim's value -
//! survive a client restart, reattach to a still-running process - is already PDN's session registry.
//! This test turns that claim into something that can fail.
//!
//! What is proven, in the order the mechanism actually works:
//!   1. `run --detach` starts a session and the registry discovers it (no address is remembered);
//!   2. the session's UNIX socket exists at the path the registry derives from the recorded pid;
//!   3. connecting to that socket reaches the SAME guest shell that the detach started - a shell that
//!      waits for a line and echoes it back, so the answer cannot come from a fresh shell;
//!   4. after the exchange the session is still the one the registry lists (identity, not just "a"
//!      session), and liveness is decided by pid + start-time + not-a-zombie.
//!
//! Why not drive `attach` itself: it copies the terminal in raw mode, which needs a TTY, and a test's
//! stdio is a pipe. The socket IS what attach copies onto, so testing the socket tests the reattach
//! mechanism and not a stand-in for it.
//!
//! Container-gated: without a container whose guest can run `/bin/sh`, this prints one named line and
//! returns - a skip is not a pass, and the line names what could not be reached.
```

## `crates/podnext/tests/session_registry.rs`  768 lines

```
//! The session registry: one reader, three renderings, and the liveness/exit
//! rules behind them (WORKLIST A2, A6, A9, B13).
//!
//! Measured before this file existed (parent-confirmed, one detached session,
//! three commands at the same instant):
//!
//! ```text
//! ps          -> sess-8449 | pdn-pv-sess | /bin/sleep 40 | 8449 | Running
//! sessions    -> only the attach hint; no session listed
//! ps -f json  -> []
//! ```
//!
//! and after `kill <c>` + `remove <c> --force` both returned 0, the store still
//! held `~/.podnext/sessions/8449.log` (0 B) and `8449.sock` (a unix socket)
//! with no process owning them.
//!
//! Every test here drives the real binary (`CARGO_BIN_EXE_podnext`). The
//! hermetic ones point the whole store at a temp root with `PN_TEST_ROOT`, so
//! they neither read nor write the real `~/.podnext`. The one that needs a
//! guest skips with a printed reason when no container is installed: an
//! unrunnable test is not a pass.
```

## `crates/podnext/tests/stdout_discipline.rs`  396 lines

```
//! WORKLIST B9 and B10: stdout is the data channel, and a page belongs to the
//! command that owns it.
//!
//! B9 - the comparator is `login/message.py:207`: the one human write in that
//! package goes to `stderr`, so `stdout` stays parseable. This file gates the
//! same rule over every sub-command that publishes both renderings (one that
//! declares its own `--format`): the JSON path's `stdout` parses as JSON, and the
//! human path's `stdout` carries data only - no `Error:`, no `Note:`, no progress
//! line. Both are measured against the built binary, not restated from the source.
//!
//! B10 - `podnext help <cmd>` must answer with that command's page while the
//! configuration is unreadable, and a bad flag must print that command's page
//! rather than a bare `Usage:` blob.
//!
//! Every process runs in its own sandbox under `$TMPDIR`/`$PREFIX/tmp`: `HOME`,
//! `PN_TEST_ROOT`, `TERMUX__HOME` and `TMPDIR` all point there, `XDG_RUNTIME_DIR`
//! is removed (it outranks `HOME` for the daemon socket), and the operator store
//! is never touched. `PN_TEST_ROOT` is a HOME-like base - the product composes
//! `<PN_TEST_ROOT>/.podnext/containers` under it - never a store root.
//!
//! Ceiling of the human-path assertion: it is a denylist of the message prefixes
//! the product uses (`Error:`, `Note:`, `Installing`, ...) plus non-emptiness, not
//! a parser for "data". A command that invents a new diagnostic prefix would slip
//! through until the prefix is added here.
```

## `crates/podnext/tests/stub_refusals.rs`  157 lines

```
//! Roadmap section 9.1: a command that cannot do its job must refuse, name the reason, and exit
//! non-zero. `build submit|status|cancel|list|artifacts|logs` and `watchdog events` used to print
//! "(not yet implemented)" on stdout and return `Ok(())` — exit 0 for something that did nothing,
//! which a script cannot distinguish from success.
```

# crates/proot-compat


## `crates/proot-compat/src/cap_sys_admin.rs`  569 lines

```
//! CAP_SYS_ADMIN capability detection and filesystem mounting with fallbacks
//!
//! This module provides a robust architecture for handling filesystem mounts
//! on Android/Termux where CAP_SYS_ADMIN may not be available. It detects
//! capability availability and provides graceful fallbacks.
```

Public items - first doc line, `-` when undocumented:

- `enum CapSysAdminStatus`: -
- `struct MountConfig`: -
- `struct MountFlags`: -
- `fn to_nix_flags`: -
- `struct CapSysAdminManager`: CAP_SYS_ADMIN manager for filesystem operations
- `fn new`: Create a new CAP_SYS_ADMIN manager with fallback mode enabled by default. This manager is safe for environments where CAP_SYS_ADMIN is not available, for example
- `fn new_with_capability_testing`: Create a new manager with capability testing enabled Use this only in environments where you know CAP_SYS_ADMIN is available
- `fn new_fallback`: Create a new manager with forced fallback mode
- `fn check_capability`: Check if CAP_SYS_ADMIN is available
- `fn status`: Get current capability status
- `fn is_fallback_mode`: Check if fallback mode is active
- `fn mount_with_fallback`: Mount a filesystem with fallback to directory creation
- `fn mount_essential_filesystems`: Mount the essential filesystems proc, sysfs, devtmpfs and tmpfs, and use fallbacks.
- `fn setup_cgroup_v2`: Setup cgroup v2 with fallback
- `fn bind_mount_with_fallback`: Bind mount with fallback
- `fn unmount`: Unmount filesystem
- `fn init_global_manager`: Initialize global CAP_SYS_ADMIN manager with fallback mode by default Use `init_global_manager_with_testing(true)` to enable capability testing
- `fn init_global_manager_with_testing`: Initialize the global manager with capability testing enabled for integration tests.
- `fn global_manager`: Get global CAP_SYS_ADMIN manager
- `fn global_manager_mut`: Get mutable global CAP_SYS_ADMIN manager
- `fn is_cap_sys_admin_available`: Check if CAP_SYS_ADMIN is available globally
- `fn is_fallback_mode`: Check if fallback mode is active globally

## `crates/proot-compat/src/cgroup_v2.rs`  42 lines

```
//! Cgroup v2 support for container isolation
//! Setup cgroup v2 mount dan konfigurasi di dalam guest rootfs
```

Public items - first doc line, `-` when undocumented:

- `fn setup_cgroup_v2`: Setup cgroup v2 inside the guest rootfs
- `fn cgroup_v2_available`: Check if cgroup v2 is available in the rootfs.

## `crates/proot-compat/src/lib.rs`  56 lines

```
//! PRoot Compat - Compatibility layer for PRoot on Termux Android 16+
//!
//! This crate provides compatibility layers for running PRoot on Termux Android 16+,
//! addressing common issues like:
```

Public items - first doc line, `-` when undocumented:

- `fn init_compat`: Initialize all compatibility layers for PRoot on Termux Android 16+
- `fn patch_rootfs`: Apply all patches to a guest rootfs directory.

## `crates/proot-compat/src/mount_helper.rs`  57 lines

```
//! Mount helper for essential filesystems
//! Mount otomatis /proc, /sys, /dev, /run, /tmp di dalam guest rootfs
```

Public items - first doc line, `-` when undocumented:

- `fn mount_essential_filesystems`: Mount essential filesystems inside the guest rootfs
- `fn unmount_essential_filesystems`: Unmount all essential filesystems for cleanup.
- `fn bind_mount`: Bind mount a host path into the guest rootfs

## `crates/proot-compat/src/path_translation.rs`  115 lines

```
//! Path translation for symlink chain resolution
//! Resolusi lengkap dari symlink chains seperti:
//! /usr/local/bin/chromium -> /usr/local/bin/browser -> /opt/browser/chrome
```

Public items - first doc line, `-` when undocumented:

- `fn resolve_symlink_chain`: Resolve a symlink chain fully, following all intermediate symlinks
- `fn is_symlink`: Check if a path is a symlink.
- `fn translate_guest_to_host`: Translate a path from guest to host perspective
- `fn translate_host_to_guest`: Translate a path from host to guest perspective

## `crates/proot-compat/src/platform_shim.rs`  64 lines

```
//! Platform personality shim
//! Mengubah uname() untuk melaporkan "Linux" bukan "Android"
//! Memastikan process.platform = "linux" di Node.js/npm
```

Public items - first doc line, `-` when undocumented:

- `fn set_linux_personality`: Set personality to Linux for Android guests
- `fn get_platform_info`: Get current platform information.

## `crates/proot-compat/src/seccomp_bypass.rs`  64 lines

```
//! Seccomp filter bypass for Android
//! Menambahkan Android-specific syscalls yang diizinkan
//! agar tidak double-block oleh seccomp filter PRoot
```

Public items - first doc line, `-` when undocumented:

- `fn configure_seccomp_filter`: Configure seccomp filter to allow Android-specific syscalls
- `fn get_android_allowed_syscalls`: Get the list of Android-specific syscalls that are allowed.

## `crates/proot-compat/src/security.rs`  977 lines

```
//! Security hardening for PRoot containers
//!
//! Provides seccomp profile management, AppArmor/SELinux integration,
//! and security policy enforcement for containers.
```

Public items - first doc line, `-` when undocumented:

- `struct SecurityProfileConfig`: -
- `enum SecurityProfileType`: -
- `struct SeccompProfile`: -
- `enum SeccompAction`: -
- `enum SeccompArch`: -
- `struct SeccompSyscallRule`: -
- `struct SeccompArg`: -
- `enum SeccompOp`: -
- `struct AppArmorProfile`: -
- `struct SelLinuxProfile`: -
- `struct CapabilityConfig`: -
- `struct SecurityHardeningManager`: Security hardening manager
- `fn new`: -
- `fn with_custom_dirs`: Create a new SecurityHardeningManager with custom directory paths. Useful for testing.
- `fn register_profile`: Register a security profile
- `fn get_profile`: Get a security profile
- `fn apply_profile`: Apply security profile to container
- `fn generate_default_seccomp_profile`: Generate default seccomp profile for containers
- `fn generate_restricted_seccomp_profile`: Generate restricted seccomp profile
- `fn generate_unconfined_seccomp_profile`: Generate unconfined seccomp profile

## `crates/proot-compat/src/stub_fs.rs`  58 lines

```
//! Stub filesystem creation
//! Membuat stub file/direktori di dalam guest rootfs untuk kompatibilitas Chrome headless
//! dan aplikasi lain yang membutuhkan /run/dbus, /sys/class, dll.
```

Public items - first doc line, `-` when undocumented:

- `fn create_stub_filesystem`: Create all stub directories and files needed for PRoot guest compatibility.
- `fn stub_exists`: Check if a stub path exists in the rootfs.

# crates/proot-core


## `crates/proot-core/src/arch.rs`  205 lines

```
//! Architecture definitions for PRoot
```

Public items - first doc line, `-` when undocumented:

- `enum Arch`: -
- `fn from_host`: -
- `fn to_str`: -
- `fn uname_machine`: -
- `fn pointer_size`: -
- `fn is_64bit`: -
- `fn supports_32bit`: -
- `fn get_arch`: Get the host architecture
- `struct Sysnum`: -
- `const fn`: -
- `const SYS_READ`: -
- `const SYS_WRITE`: -
- `const SYS_OPEN`: -
- `const SYS_CLOSE`: -
- `const SYS_STAT`: -
- `const SYS_FSTAT`: -
- `const SYS_LSTAT`: -
- `const SYS_POLL`: -
- `const SYS_LSEEK`: -
- `const SYS_MMAP`: -
- `const SYS_MPROTECT`: -
- `const SYS_MUNMAP`: -
- `const SYS_BRK`: -
- `const SYS_RT_SIGACTION`: -
- `const SYS_RT_SIGPROCMASK`: -
- `const SYS_IOCTL`: -
- `const SYS_PREAD64`: -
- `const SYS_PWRITE64`: -
- `const SYS_READV`: -
- `const SYS_WRITEV`: -
- `const SYS_ACCESS`: -
- `const SYS_PIPE`: -
- `const SYS_SELECT`: -
- `const SYS_SCHED_YIELD`: -
- `const SYS_MREMAP`: -
- `const SYS_MSYNC`: -
- `const SYS_MINCORE`: -
- `const SYS_MADVISE`: -
- `const SYS_SHMGET`: -
- `const SYS_SHMAT`: -
- `const SYS_SHMCTL`: -
- `const SYS_DUP`: -
- `const SYS_DUP2`: -
- `const SYS_PAUSE`: -
- `const SYS_NANOSLEEP`: -
- `const SYS_GETITIMER`: -
- `const SYS_ALARM`: -
- `const SYS_SETITIMER`: -
- `const SYS_GETPID`: -
- `const SYS_SENDFILE`: -
- `const SYS_SOCKET`: -
- `const SYS_CONNECT`: -
- `const SYS_ACCEPT`: -
- `const SYS_SENDTO`: -
- `const SYS_RECVFROM`: -
- `const SYS_SENDMSG`: -
- `const SYS_RECVMSG`: -
- `const SYS_SHUTDOWN`: -
- `const SYS_BIND`: -
- `const SYS_LISTEN`: -
- `const SYS_GETSOCKNAME`: -
- `const SYS_GETPEERNAME`: -
- `const SYS_SOCKETPAIR`: -
- `const SYS_SETSOCKOPT`: -
- `const SYS_GETSOCKOPT`: -
- `const SYS_CLONE`: -
- `const SYS_FORK`: -
- `const SYS_VFORK`: -
- `const SYS_EXECVE`: -
- `const SYS_EXIT`: -
- `const SYS_WAIT4`: -
- `const SYS_KILL`: -
- `const SYS_UNAME`: -
- `const SYS_GETUID`: -
- `const SYS_GETGID`: -
- `const SYS_GETEUID`: -
- `const SYS_GETEGID`: -
- `const SYS_SETUID`: -
- `const SYS_SETGID`: -
- `const SYS_GETPPID`: -
- `const SYS_GETPGRP`: -
- `const SYS_SETSID`: -
- `const SYS_GETPGID`: -
- `const SYS_SETGROUPS`: -
- `const SYS_GETGROUPS`: -
- `const SYS_GETRLIMIT`: -
- `const SYS_SETRLIMIT`: -
- `const SYS_GETRUSAGE`: -
- `const SYS_GETTIMEOFDAY`: -
- `const SYS_SETTIMEOFDAY`: -
- `const SYS_RSEQ`: -
- `const SYS_MEMBARRIER`: -
- `const SYS_PROCESS_VM_READV`: -
- `const SYS_PROCESS_VM_WRITEV`: -
- `const SYS_CLONE3`: -
- `const SYS_OPENAT2`: -
- `const SYS_MLOCK`: -
- `const SYS_MUNLOCK`: -
- `const SYS_ARCH_PRCTL`: -
- `const SYS_PRLIMIT64`: -
- `const SYS_FACCESSAT2`: -
- `const SYS_STATX`: -
- `const SYS_IO_PGETEVENTS`: -
- `const SYS_PIDFD_SEND_SIGNAL`: -
- `const SYS_PIDFD_OPEN`: -
- `const SYS_PIDFD_GETFD`: -
- `const SYS_PROCESS_MADVISE`: -
- `const SYS_EPOLL_PWAIT2`: -
- `const SYS_MOUNT_SETATTR`: -
- `const SYS_QUOTACTL_FD`: -
- `const SYS_LANDLOCK_CREATE_RULESET`: -
- `const SYS_LANDLOCK_ADD_RULE`: -
- `const SYS_LANDLOCK_RESTRICT_SELF`: -
- `const SYS_MEMFD_SECRET`: -
- `const SYS_PROCESS_MRELEASE`: -
- `const SYS_FUTEX_WAITV`: -
- `const SYS_SET_MEMPOLICY_HOME_NODE`: -
- `const SYS_CACHEDLCTL`: -
- `const SYS_SCHED_SETATTR`: -
- `const SYS_MOVE_MOUNT`: -
- `const SYS_FSOPEN`: -
- `const SYS_FSCONFIG`: -
- `const SYS_FSMOUNT`: -
- `const SYS_FSPICK`: -
- `const SYS_MOVE_PAGES`: -

## `crates/proot-core/src/cli.rs`  147 lines

```
//! CLI configuration for PRoot
```

Public items - first doc line, `-` when undocumented:

- `struct ProotConfig`: -
- `fn parse_bind_mounts`: -
- `fn parse_env`: -
- `struct BindMount`: -

## `crates/proot-core/src/compat.rs`  86 lines

```
//! Compatibility layer for PRoot
```

Public items - first doc line, `-` when undocumented:

- `fn resolve_symlink_chain`: Resolve symlink chain with a maximum depth to prevent infinite loops
- `fn get_personality`: Platform-specific personality/spoofing
- `const ALLOWED_ANDROID_SYSCALLS`: Seccomp bypass for Android syscalls

## `crates/proot-core/src/extension.rs`  188 lines

```
//! Extension system for PRoot
```

Public items - first doc line, `-` when undocumented:

- `trait Extension`: Extension trait for PRoot extensions
- `struct ExtensionManager`: Extension manager
- `fn new`: -
- `fn register`: -
- `fn init_all`: -
- `fn handle_syscall_entry_all`: -
- `fn handle_syscall_exit_all`: -
- `struct PathTranslationExtension`: Path translation extension
- `struct VFSExtension`: VFS extension
- `struct SeccompExtension`: Seccomp extension

## `crates/proot-core/src/identity.rs`  84 lines

```
//! The identity and credential syscall table (adoption S2).
//!
//! `shroot` decides guest identity with an explicit table of syscalls rather than a single
//! mapping: `native/shroot/src/interception.c:485-491` defines an `IDENTITY(n)` macro and lists
//! the calls that decide identity. Read raw at `shroot` commit `7ab99d6`, that list holds
//! **18** syscalls, `getuid` through `capset`. (PDN's worklist recorded 19; the source says 18,
//! and the rows below are the source's eighteen.) PDN's runtime maps every guest id to one host
//! uid (`--change-id=0:0:0`, `crates/proot-core/src/lib.rs`), so what a guest sees for each of
//! those calls is fixed - but until now it was named nowhere: it was implicit in the runtime's
//! argv, visible only to a reader who already knew where to look.
//!
//! This module is that named place. One row per syscall that can change or report a guest's
//! identity, each with what PDN does about it. The count is written down once, as
//! `IDENTITY_SYSCALL_COUNT`, and pinned to the rows by a compile-time assertion and by
//! `tests/identity_table.rs`, so the table and its own count cannot drift apart - the defect this
//! project keeps finding in tables whose comment disagrees with their contents.
//!
//! The split is by the call's stated function: `getuid`, `getgroups`, `capget` and their
//! siblings *report* identity, so a guest is answered with the configured identity; `setuid`,
//! `setgroups`, `capset` and their siblings *change* it, and PDN adds no policy for them - the
//! runtime takes them, and per the rule carried from `shroot` (`docs/shroot.md`: a denied
//! operation never switches identity) a change that does not take effect never re-identifies the
//! process. `setfsuid` and `setfsgid` also return the previous value; they are grouped with the
//! changes because changing the filesystem identity is what the call is for.
```

Public items - first doc line, `-` when undocumented:

- `enum IdentityDisposition`: -
- `const IDENTITY_SYSCALL_TABLE`: The syscalls that can change or report a guest's identity, in `shroot`'s order, given in `native/shroot/src/interception.c:487-491`, each with what PDN does a
- `const IDENTITY_SYSCALL_COUNT`: The number of rows in [`IDENTITY_SYSCALL_TABLE`], written down once so the table, this documentation and `tests/identity_table.rs` all cite the same n
- `fn identity_disposition`: What PDN does about `name`, or `None` when it is not one of the syscalls PDN names as an identity or credential call

## `crates/proot-core/src/lib.rs`  679 lines

```
//! PRoot Core - Rust Implementation
//!
//! A ptrace-based chroot, mount --bind, and binfmt_misc implementation
//! without privilege/setup for Linux. Compatible with Termux Android 16+.
```

Public items - first doc line, `-` when undocumented:

- `struct ChildExit`: -
- `struct ProotEngine`: -
- `fn new`: -
- `fn run`: -
- `fn start`: Start a daemon process in background mode.
- `fn execute`: Execute a command in the foreground and wait for completion, returning exit code
- `fn execute_selective`: Run the configured command through the selective-interception backend (S1) instead of PRoot
- `fn shutdown`: -
- `fn init`: Initialize PRoot core with default configuration
- `fn exit_status_code`: The exit code that a completed wait maps onto. See NEXT_DEV section 4 for the runc contract.

## `crates/proot-core/src/path.rs`  69 lines

```
//! Path translation for PRoot
```

Public items - first doc line, `-` when undocumented:

- `struct PathTranslator`: Path translator for guest/host path translation
- `fn new`: -
- `fn translate_guest_to_host`: -
- `fn translate_host_to_guest`: -
- `fn translate_syscall_args`: -
- `fn translate_syscall_retval`: -
- `fn resolve_symlink_chain`: -

## `crates/proot-core/src/ptrace.rs`  196 lines

```
//! Ptrace controller for PRoot
```

Public items - first doc line, `-` when undocumented:

- `enum PtraceEvent`: -
- `fn from_wait_status`: -
- `struct PtraceController`: Ptrace controller managing all tracees
- `fn new`: -
- `fn add_tracee`: -
- `fn remove_tracee`: -
- `fn get_tracee`: -
- `fn get_all_tracees`: -
- `fn run_loop`: -
- `fn shutdown`: -

## `crates/proot-core/src/seccomp.rs`  211 lines

```
//! Seccomp filter for PRoot.
//!
//! This installs a real seccomp BPF filter. The previous `install()` set
//! `PR_SET_NO_NEW_PRIVS` and then did nothing, so callers held a security
//! boundary that did not exist (WORKLIST A21).
//!
//! The baseline denies syscalls a guest never needs and that are classic
//! container-escape / kernel-loading primitives: `open_by_handle_at` and
//! `kexec_load`. `ptrace`, `process_vm_readv` and `process_vm_writev` are
//! deliberately NOT denied, and that is measured, not assumed. The filter is
//! installed in this process and seccomp filters are inherited across `execve`,
//! so it also governs the `proot` binary this engine spawns, whose whole
//! mechanism is `ptrace(PTRACE_TRACEME)`. On aarch64 Termux:
//!
//! - denying `ptrace` makes proot abort: `proot error: ptrace(TRACEME): Operation
//!   not permitted`;
//! - denying `process_vm_readv` / `process_vm_writev` makes proot's file I/O fail
//!   with EFAULT: `sh: cannot create ...: Bad address`;
//! - denying `open_by_handle_at` and `kexec_load` leaves proot working.
//!
//! Denying a boundary that breaks the tool it protects is not hardening, so the
//! filter keeps the two the product does not need and allows the rest.
```

Public items - first doc line, `-` when undocumented:

- `const COOKIE_PATH`: Per-category cookies for a `SECCOMP_RET_TRACE` decision (S1)
- `const COOKIE_MEMORY`: -
- `const COOKIE_PROCESS`: -
- `fn cookie_category`: Name the category a `TRACE` cookie selects, or `None` for a cookie this build does not stamp
- `struct SeccompFilter`: Seccomp filter for PRoot
- `fn new`: -
- `fn install`: Install the filter into the calling process
- `fn add_allowed_syscall`: -
- `fn add_denied_syscall`: -
- `fn install_selected_trace`: Install the selective-interception filter into the calling process (S1)

## `crates/proot-core/src/selective.rs`  246 lines

```
//! Selective seccomp interception backend (S1).
//!
//! The second execution backend: a `SECCOMP_RET_TRACE` filter over a *selected*
//! syscall set ([`selected_syscalls`]) with `SECCOMP_RET_ALLOW` for everything
//! else, and one tracer that reads each stop's cookie through the existing
//! `PtraceEvent::Seccomp` handler (`tracee.rs`). Everything not selected runs
//! native, with no stop — the model the reference implementation uses (shroot's
//! `ADAPT`/`CONTROL` macros, `fd_filter_program.c:66-80`) and the one PRoot
//! cannot be, because PRoot stops on every syscall.
//!
//! Scope, stated plainly: the command is `execve`'d as-is. There is no guest
//! root, no path translation and no bind mounts, so this is the interception
//! mechanism measured end to end, not a drop-in replacement for `proot`.
```

Public items - first doc line, `-` when undocumented:

- `fn selected_syscalls`: The selected set: the syscalls a PDN guest needs adapted, each with the cookie naming its category
- `struct SelectedRun`: -
- `fn intercepted`: How many syscalls stopped the tracer.
- `fn count`: How many stops carried this category cookie.
- `fn run_traced`: Run `body` in a child under the selective filter, with this process as its tracer
- `fn run_selected_cmd`: Run `argv` under the selective backend: the child `execve`s it, so the guest is a normal native process that only stops on the selected syscalls.

## `crates/proot-core/src/syscall.rs`  1395 lines

```
//! System call handler for PRoot
```

Public items - first doc line, `-` when undocumented:

- `trait SyscallHandlerFn`: Trait for system call handler functions
- `struct SyscallHandler`: System call handler
- `struct SyscallConfig`: -
- `fn new`: -
- `fn handle_entry`: -
- `fn handle_exit`: -

## `crates/proot-core/src/tracee.rs`  327 lines

```
//! Tracee management for PRoot
```

Public items - first doc line, `-` when undocumented:

- `struct TraceeId`: -
- `enum TraceeState`: -
- `struct Tracee`: -
- `fn new`: -
- `fn handle_syscall_entry`: -
- `fn handle_syscall_exit`: -
- `fn handle_event`: -
- `struct RawSyscall`: -
- `struct SyscallInfo`: -
- `struct Registers`: -
- `struct PathInfo`: -
- `struct MountInfo`: -
- `struct VfsContext`: -
- `struct OpenFile`: -
- `struct FdTable`: -
- `fn new`: -
- `fn allocate`: -
- `fn get`: -
- `fn get_mut`: -
- `fn close`: -
- `struct FdEntry`: -

## `crates/proot-core/src/tracee_state.rs`  135 lines

```
//! Tracee state management for PRoot
```

Public items - first doc line, `-` when undocumented:

- `struct TraceeStateManager`: Tracee state manager
- `struct StateEvent`: -
- `fn new`: -
- `fn add_tracee`: -
- `fn remove_tracee`: -
- `fn get_tracee`: -
- `fn get_all_tracees`: -
- `fn update_state`: -
- `fn update_syscall_info`: -
- `fn update_path_info`: -
- `fn record_state_change`: -
- `fn get_tracee_count`: -
- `fn get_running_tracees`: -
- `fn get_syscall_stats`: -

## `crates/proot-core/src/vfs.rs`  133 lines

```
//! Virtual File System (VFS) manager for PRoot
```

Public items - first doc line, `-` when undocumented:

- `struct VfsManager`: VFS Manager for PRoot
- `struct MountPoint`: -
- `fn new`: -
- `fn initialize`: -
- `fn translate_path`: -
- `fn resolve_path`: -
- `fn check_access`: -
- `fn get_next_fd`: -
- `fn get_rootfs`: -
- `fn get_bind_mounts`: -

## `crates/proot-core/tests/argv_facts.rs`  160 lines

```
//! What `proot-core` actually puts in the `proot` argument vector.
//!
//! Workbench WORKLIST B6/B7. The argv builder is inline in
//! `ProotEngine::{start,execute}` (`crates/proot-core/src/lib.rs:200-299`,
//! `:338-457`); there is no function returning an argv the code also reports on
//! itself. So this test observes the argv the way `proot` receives it:
//! `proot_binary()` honours `PROOT_BIN` (`crates/proot-core/src/lib.rs:76-90`), a
//! capture script is installed there, and the real `execute()` path is run. The
//! script records `"$@"` and the guest environment; the test reads both back.
//! Nothing here is a boolean the builder returns about its own behaviour.
```

## `crates/proot-core/tests/identity_table.rs`  98 lines

```
//! Adoption S2: the identity/credential syscall table must be countable, complete and honest.
//!
//! `shroot` lists the syscalls that decide guest identity in one table
//! (`native/shroot/src/interception.c:485-491`). PDN's runtime maps every guest id to one host uid
//! and named that set nowhere; `proot_core::identity` is the named place, and this test is what
//! keeps its documentation and its rows from disagreeing. It counts the rows against the
//! documented number, checks the rows against an independent statement of them (a table compared
//! against a count read from itself proves nothing), and asserts that a syscall *outside* the
//! table is not silently claimed to be handled.
```

## `crates/proot-core/tests/seccomp_filter.rs`  71 lines

```
//! WORKLIST A21: `install()` must install a filter that denies syscalls, or the
//! caller must be told it did not.
//!
//! The old `install()` set `PR_SET_NO_NEW_PRIVS` and then did nothing, so a
//! caller held a security boundary that did not exist. This observes a real
//! denial instead of trusting a return value: the filter is irreversible and
//! inherited by children, so it is installed in a forked child, and the child
//! reports through its exit code whether a denied syscall was denied with `EPERM`
//! and an allowed one still worked.
//!
//! The probe denies `getpid`. That is deliberate: `open_by_handle_at` and
//! `kexec_load` already fail with `EPERM` without privilege, so their `EPERM`
//! proves nothing about the filter; `getpid` can never fail unfiltered.
```

## `crates/proot-core/tests/selective_seccomp.rs`  168 lines

```
//! S1: the selective-interception backend, exercised and measured against the
//! PRoot path on the same workload.
//!
//! `proot_core::selective` is the second execution backend: a `SECCOMP_RET_TRACE`
//! filter over a selected syscall set, everything else `SECCOMP_RET_ALLOW`, and a
//! tracer that reads each stop's cookie through the existing `PtraceEvent::Seccomp`
//! handler. The correctness test asserts the shape of the interception; the
//! measurement prints both wall times for one workload on this device.
```

# crates/proot-distro-core


## `crates/proot-distro-core/src/android_binds.rs`  653 lines

```
//! The Android bind set a session is given — and the termux-type exclusion.
//!
//! A guest's dynamic linker needs a few host directories (`/apex`, `/system`,
//! `/vendor`, the `linkerconfig` files) and PDN used to hand them over
//! unconditionally, from a hard-coded argument list under a comment
//! (`proot-core`'s `add_android_system_binds`). The reference instead *decides*:
//! each path is resolved, missing paths are dropped, a directory is bound only
//! when the guest could traverse it, and a file only when it can be read
//! (`proot_distro/commands/login/bindings.py:system_bindings`). It also withholds
//! the whole set from a **termux-type** rootfs — one that carries its own
//! `$PREFIX/bin/login` — because such a guest is a Termux installation of its
//! own and shares the host's `$PREFIX` path, so a host bind would let the host's
//! binaries appear inside it (`proot_cmd.py`, the `dist_type != "termux"` guards).
//!
//! Each clause here is a separate filter with its own test at the bottom of this
//! file, so the rule is *shown*, not asserted in a comment:
//!
//! * **mode digit** — a directory is bound only when the last digit of its mode
//!   is world-searchable (`1`, `5` or `7`). The reference skips the rest, since
//!   an untraversable bind is not a bind.
//! * **readability** — a file is bound only when a 1-byte read succeeds.
//! * **termux-type** — a rootfs whose *guest walk* finds `$PREFIX/bin/login` gets
//!   none of this set. The walk never follows a symlink, so a guest-planted
//!   `data -> /` cannot make PDN answer for the host's own Termux login.
//!
//! The filesystem is reached through [`HostFs`] so every branch is driven from a
//! constructed answer in a test, not from whatever this device's `/apex` happens
//! to be.
```

Public items - first doc line, `-` when undocumented:

- `const SYSTEM_PATHS`: The paths the reference probes, in its order.
- `const STORAGE_FALLBACKS`: The reference's `storage_bindings()` fallbacks, in its order: the first readable one is mapped onto all four documented guest paths.
- `const STORAGE_GUEST_PATHS`: The four guest paths that the reference maps storage onto, in reference order.
- `const SKIP_STORAGE`: Emitted when the host grants no readable Android storage at all.
- `const SKIP_TERMUX`: Emitted when the whole set is withheld from a termux-type rootfs.
- `const SKIP_ISOLATED`: Emitted when the whole set is withheld from an `--isolated` session.
- `enum DistType`: -
- `struct Stat`: -
- `trait HostFs`: Filesystem access, split out so a test can drive every clause.
- `struct RealFs`: The real host filesystem.
- `struct Plan`: -
- `fn plan`: Decide the Android bind set for a session.
- `fn live`: [`plan`] against the real host, with the distro type detected from `rootfs`.
- `fn extend_binds`: Add the Android bind set to a rendered `--bind` list, tracing each withheld bind
- `fn detect_dist_type`: `Termux` iff `rootfs`'s guest walk finds `$PREFIX/bin/login`.
- `fn guest_file_exists`: Does `rootfs/rel` exist as a regular file, asked of the guest walk?  Every component is opened with `O_NOFOLLOW` relative to the previous one, so a sy

## `crates/proot-distro-core/src/archive.rs`  375 lines

```
//! Install a rootfs from a local archive: a plain rootfs tarball, or an OCI
//! image-layout tar (roadmap section 10.1).
//!
//! Two shapes of input share one destination - `unpack into a clean rootfs`:
//!
//! * a **plain rootfs tarball** (`.tar`, `.tar.gz`, `.tar.xz`, `.tar.bz2`,
//!   `.tar.zst`, including a wrapping directory), extracted with the same
//!   Android-safe walker the registry path uses ([`crate::untar`]);
//! * an **OCI image layout** stored inside a tar (`oci-layout` + `index.json` +
//!   `blobs/sha256/*`), whose blobs are read by digest and **verified against
//!   the digest the manifest declares** before they are applied.
//!
//! The OCI layout needs random access (a layer blob is read by the digest a
//! manifest names, in any order), so it is read from a plain, seekable tar. A
//! compressed archive is decoded to a temporary copy first; an uncompressed one
//! is read in place. This is the same boundary the reference draws
//! (`install_local.py:install_from_local_file`, which reopens a seekable handle
//! once it sees the `oci-layout` marker), and the extraction itself is
//! reimplemented against the OCI/Docker specifications, not transcribed.
```

Public items - first doc line, `-` when undocumented:

- `fn install_from_archive`: Fill `rootfs` from the archive at `archive`
- `fn sha256_file`: The sha256 of a whole file, lowercase hex
- `fn apply_layer_from_store`: Apply one stored layer blob to `rootfs`, decoding it by the media type the manifest declares
- `fn materialize_from_store`: Materialise a rootfs from layer blobs that are already in the store

## `crates/proot-distro-core/src/bindings.rs`  189 lines

```
//! Bindings management for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `struct BindingManager`: Bindings manager
- `fn new`: -
- `fn add_bind`: -
- `fn remove_bind`: -
- `fn get_binds`: -
- `fn clear`: -
- `struct BindMount`: -
- `struct BindConfig`: -
- `struct TmpfsMount`: -
- `struct DeviceMount`: -
- `fn ensure_bindable`: Refuse a bind source that PRoot cannot express
- `fn argument`: Build the `<source>:<target>` bind argument that PRoot understands, and refuse anything that PRoot will silently drop or misplace.
- `fn to_argument`: Render this mount as the `--bind` argument PRoot expects

## `crates/proot-distro-core/src/blobstore.rs`  207 lines

```
//! A content-addressed store for OCI layer blobs.
//!
//! Until this module existed, an install verified each layer and then threw the
//! bytes away: the container was a freshly unpacked tree, and two images sharing
//! a base layer wrote that base twice. The reference keeps every blob under the
//! digest that names it, so a second image that references the same layer adds
//! nothing (moby `core/content/content.go:41-46`, the `blobs/<algo>/<hex>` tree).
//!
//! Two mechanisms are ported here:
//!
//! * **content addressing** - the blob's sha256 *is* its path, so identical bytes
//!   from any source coalesce onto one file and a re-ingest is a no-op;
//! * **two-phase ingest** - [`BlobStore::put_reader`] writes a temp, hashes it,
//!   and only `rename`s it onto the digest path once the bytes match that digest.
//!   A reader therefore never observes a partial blob at a content address: a
//!   failed or interrupted ingest leaves only a `.ingest-*.tmp` that is not
//!   reachable by digest (moby `core/metadata/content.go:604-623,670-694`).
//!
//! The store root is resolved once through [`crate::paths::resolve_home`], so a
//! redirected root (`PN_TEST_ROOT`) is followed and a test never writes into the
//! operator's real `~/.podnext`.
```

Public items - first doc line, `-` when undocumented:

- `struct PutOutcome`: -
- `fn reuse_message`: The line printed when a blob was already present and was not duplicated
- `fn store_message`: The line printed when a blob was written for the first time.
- `struct BlobStore`: A digest-addressed blob directory.
- `fn parse_digest`: Check a digest is `sha256:<64 lowercase hex>` and return the hex half
- `fn new`: The store under the resolved home: `<home>/.podnext/blobs`.
- `fn at`: A store rooted at an explicit directory, for tests and for any caller that already resolved the root.
- `fn root`: -
- `fn blob_path`: `<root>/sha256/<hex>` for a validated digest.
- `fn contains`: -
- `fn open`: Open a stored blob for reading, to materialise a container from the store.
- `fn put_reader`: Ingest `reader` under `declared`, two-phase: write, verify, rename
- `fn put_bytes`: [`BlobStore::put_reader`] over an in-memory blob.

## `crates/proot-distro-core/src/compress.rs`  204 lines

```
//! Compression utilities for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `enum CompressionFormat`: -
- `struct Compressor`: Compressor utility
- `fn new`: -
- `fn compress_file`: -
- `fn decompress_file`: -
- `fn compress_directory`: High-level functions for common operations
- `fn extract_archive`: -

## `crates/proot-distro-core/src/config.rs`  320 lines

```
//! Configuration for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `enum DistroType`: -
- `struct DistroConfig`: -
- `struct Repository`: -
- `struct Scripts`: -
- `struct InstallOptions`: -
- `struct BindMount`: -
- `struct ResourceLimits`: -
- `struct NetworkConfig`: -
- `enum NetworkMode`: -
- `struct PortMapping`: -
- `struct SecurityConfig`: -
- `struct StorageConfig`: -
- `struct Config`: -
- `struct PathsConfig`: -
- `struct LoggingConfig`: -
- `struct FileRotationConfig`: -

## `crates/proot-distro-core/src/constants.rs`  114 lines

```
//! Constants for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `const DEFAULT_ROOT_DIR`: -
- `const DEFAULT_CONTAINERS_DIR`: -
- `const DEFAULT_IMAGES_DIR`: -
- `const DEFAULT_CACHE_DIR`: -
- `const DEFAULT_LOG_DIR`: -
- `const DEFAULT_CONFIG_DIR`: -
- `const DEFAULT_DISTRO`: -
- `const DEFAULT_VERSION`: -
- `const DEFAULT_ARCH`: -
- `const DEFAULT_NETWORK_BRIDGE`: -
- `const DEFAULT_SUBNET`: -
- `const DEFAULT_CPU_LIMIT`: -
- `const DEFAULT_MEMORY_LIMIT`: -
- `const DEFAULT_DISK_LIMIT`: -
- `const DEFAULT_PIDS_LIMIT`: -
- `const DEFAULT_DNS_SERVERS`: -
- `const DEFAULT_BRIDGE`: -
- `struct Constants`: -
- `const PREFIX`: -
- `const HOME`: -
- `const TMP`: -
- `const PACKAGE`: -
- `const STORAGE_PERMISSIONS`: The directories Termux grants access to after `termux-setup-storage`
- `struct TermuxConstants`: -

## `crates/proot-distro-core/src/container.rs`  1081 lines

```
//! Container management for PRoot Distro
//!
//! Provides container lifecycle management with proot-core integration.
```

Public items - first doc line, `-` when undocumented:

- `struct ContainerManager`: Container manager
- `fn new`: -
- `fn create`: -
- `fn get`: -
- `fn remove`: -
- `fn list`: -
- `fn list_sessions`: -
- `fn kill`: -
- `fn start`: -
- `fn stop`: -
- `fn restart`: -
- `fn get_backend`: Returns the execution backend corresponding to container's configured runtime
- `fn execute`: Execute a command in the container in foreground mode, and wait for completion.
- `fn execute_container`: -
- `fn backup`: -
- `fn restore`: -
- `struct ContainerConfig`: -
- `struct BindMount`: -
- `fn to_argument`: Render this mount as the `--bind` argument PRoot expects
- `struct ResourceLimits`: -
- `struct NetworkConfig`: -
- `enum NetworkMode`: -
- `struct PortMapping`: -
- `struct SecurityConfig`: -
- `struct Container`: -
- `fn new`: -
- `fn info`: -
- `struct ContainerInfo`: -
- `enum ContainerStatus`: -
- `struct SessionInfo`: -
- `fn scan_store`: Read the whole container store from disk: `read_dir` plus a full read of every container's `metadata.json` and `/etc/os-release`
- `const STAGING_PREFIX`: The prefix the install path gives a staging directory, defined in `native/image.rs` as `format!(".tmp-install-{name}-{uuid}")`.
- `enum MetadataKind`: -
- `fn metadata_kind`: Classify `<dir>/metadata.json` without ever blocking on it
- `fn read_metadata_text`: The one metadata read of the store, namely the text of `<dir>/metadata.json`, but only when that path holds a regular file.
- `fn is_container_dir`: Whether a directory in the container store describes a container

## `crates/proot-distro-core/src/container_index.rs`  289 lines

```
//! On-disk cache of the container store listing (roadmap section 9.2).
//!
//! A metadata query used to mean `read_dir` plus a full read of every container's
//! `metadata.json` and `/etc/os-release`, so its latency grew with the container
//! count. This module caches the scanned listing and reuses it while nothing in the
//! store has changed.
//!
//! The freshness key is the mtime and size of each container's `metadata.json` (the
//! directory's own mtime for legacy containers that predate that file) — the one
//! file a rebuild rewrites. A change to any of them makes the key differ and the
//! next query falls back to the full scan. `PODNEXT_NO_INDEX_CACHE=1` disables the
//! cache entirely; the benchmark harness uses it as its negative control.
//!
//! Only the *derived* fields are cached ([`ContainerInfo`]), not the whole
//! `Container`: the scan fills everything else from `ContainerConfig::default()`, and
//! reparsing the full config through JSON cost more than the file reads it replaced
//! (measured: a full-`Container` cache lost to no cache at 10, 50, 200 and 600
//! containers).
```

Public items - first doc line, `-` when undocumented:

- `const INDEX_FILE`: Cache file name, written under the store's cache directory.
- `fn enabled`: Whether the index cache is in use
- `fn load_if_fresh`: The cached listing, if the cache exists, parses, is current, and every container directory still carries the stamp it had when the entry was written
- `fn store`: Persist `entries` for `dir`

## `crates/proot-distro-core/src/dirfd.rs`  329 lines

```
//! Directory file descriptor management and hardened descriptor VFS.
```

Public items - first doc line, `-` when undocumented:

- `struct DirFd`: Hardened directory file descriptor wrapper with automated cleanup on drop.
- `fn open`: Open a directory file descriptor.
- `fn from_raw_fd`: Wrap an existing raw file descriptor.
- `fn as_raw_fd`: Get raw file descriptor.
- `fn into_raw_fd`: Consume the wrapper and return the raw fd. Do not close the fd.
- `fn path`: Associated path if available.
- `fn open_dir_at`: Open a subdirectory relative to this directory descriptor with O_NOFOLLOW | O_DIRECTORY | O_CLOEXEC.
- `fn open_regular_at`: Open a regular file relative to this directory descriptor
- `fn safe_chmod_at`: Safely modify permissions on a verified descriptor.
- `fn atomic_write_at`: Atomically write content using an exclusive temporary file followed by renameat.
- `fn read_dir`: -
- `fn open_file`: -
- `fn create_dir`: -
- `fn unlink`: -
- `fn rename`: -

## `crates/proot-distro-core/src/dockerfile.rs`  381 lines

```
//! Dockerfile parser for PDN image builds
```

Public items - first doc line, `-` when undocumented:

- `enum DockerfileInstruction`: -
- `fn parse_dockerfile`: Parse Dockerfile content into a list of instructions

## `crates/proot-distro-core/src/gpu.rs`  312 lines

```
//! GPU acceleration support for PRoot containers
//!
//! Provides device detection and passthrough for GPU acceleration
//! including Vulkan, DRM/KMS, and OpenGL ES on Android.
```

Public items - first doc line, `-` when undocumented:

- `struct GpuDevice`: -
- `enum GpuDeviceType`: -
- `struct GpuConfig`: -
- `fn detect_gpu_devices`: Detect available GPU devices on the host
- `fn generate_gpu_bind_mounts`: Generate bind mount arguments for GPU devices
- `fn apply_gpu_to_rootfs`: Apply GPU configuration to container rootfs

## `crates/proot-distro-core/src/guestdirs.rs`  110 lines

```
//! Runtime directories that live beside a container's rootfs and are bound into
//! the guest.
//!
//! Currently one: the `/dev/shm` store. PDN passed `--bind=/dev`, which masks
//! whatever the rootfs has under `dev/`, so the guest had **no** `/dev/shm` at
//! all:
//!
//! ```text
//! $ podnext login deb-pdn -- touch /dev/shm/x
//! touch: cannot touch '/dev/shm/x': No such file or directory
//! ```
//!
//! proot-distro gives every container a real, writable `shm/` sibling and binds
//! it (`proot_distro/shm.py`), which is why the same command succeeds there.
//! Anything using POSIX shared memory needs it — Chromium in particular writes
//! its shared-memory segments there (`--disable-dev-shm-usage` is a workaround,
//! not a fix).
//!
//! The directory is mode 1777: world-writable with the sticky bit, which is what
//! `/dev/shm` is on a real system.
```

Public items - first doc line, `-` when undocumented:

- `fn shm_dir`: The store's path, a sibling of the rootfs so it is removed with the container.
- `fn tmp_dir`: The isolated tmp path, a sibling of the rootfs so it is removed with the container.
- `fn ensure`: Create the stores if missing and make sure their mode is 1777.
- `fn bindings`: `--bind=<src>:<dst>` for the guest's `/dev/shm` and `/tmp`

## `crates/proot-distro-core/src/guestfile.rs`  118 lines

```
//! Guest file access for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `fn read_guest_bytes`: Read bytes from guest filesystem
- `fn write_guest_bytes`: Write bytes to guest filesystem
- `fn read_guest_string`: Read guest file as string
- `struct GuestFile`: Guest file operations
- `fn open`: -
- `fn create`: -
- `fn read`: -
- `fn write`: -
- `fn seek`: -
- `fn close`: -
- `fn path`: -

## `crates/proot-distro-core/src/image.rs`  573 lines

```
//! Image management for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `enum ImageSource`: -
- `struct ImageManager`: Image manager for container images
- `fn new`: -
- `fn download_and_verify`: -
- `fn download_and_verify_source`: -
- `fn list`: -
- `fn remove`: -
- `fn push`: Push an image to a registry
- `fn tag`: Tag an image locally
- `fn history`: Get the image history, with layers and metadata.
- `fn inspect`: Inspect image - get detailed information
- `struct ImageHistoryEntry`: -
- `struct ImageInspectInfo`: -
- `struct ImageConfigInfo`: -
- `struct RootFSInfo`: -
- `struct Image`: -
- `struct Layer`: -
- `struct LocalImageManifest`: -
- `struct ImageConfig`: -
- `struct RootFS`: -
- `struct ContainerConfig`: -
- `struct PortInfo`: -
- `struct VolumeInfo`: -

## `crates/proot-distro-core/src/install.rs`  66 lines

```
//! Installer for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `struct Installer`: Installer for PRoot Distro
- `fn new`: -
- `fn install`: -

## `crates/proot-distro-core/src/l2s.rs`  167 lines

```
//! Link2Symlink Tree Rewriter for PRoot containers.
//!
//! Repairs intermediate hard-link symlinks when containers are renamed or relocated.
```

Public items - first doc line, `-` when undocumented:

- `fn rewrite_l2s_tree`: Recursively rewrite all link2symlink targets under `rootfs` from `old_rootfs` to `new_rootfs`

## `crates/proot-distro-core/src/layer_diff.rs`  328 lines

```
//! OCI layer differ for PDN container image builder
```

Public items - first doc line, `-` when undocumented:

- `fn compute_layer_diff`: Computes the diff between two rootfs directories and outputs an OCI-compliant tar.gz layer

## `crates/proot-distro-core/src/lib.rs`  280 lines

```
//! PRoot Distro Core - Container management for PRoot
```

Public items - first doc line, `-` when undocumented:

- `struct ProotDistroCore`: Main entry point for PRoot Distro Core
- `fn new`: -
- `fn install`: -
- `fn login`: -
- `fn remove`: -
- `fn list`: -
- `fn backup`: -
- `fn restore`: -
- `fn network_manager`: -
- `fn storage_manager`: -
- `fn init`: Initialize PRoot Distro Core

## `crates/proot-distro-core/src/locking.rs`  398 lines

```
//! Locking mechanism for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `struct LockManager`: Lock manager for containers
- `fn new`: -
- `fn acquire`: -
- `fn release`: -
- `fn try_acquire`: -
- `struct FileLock`: File-based lock for cross-process synchronization
- `fn new`: -
- `fn lock`: -
- `fn unlock`: -
- `fn lock_path`: Where the exclusive lock for a container `name` lives
- `struct ContainerLock`: Container lock for exclusive access
- `fn new`: The lock for `name` at its default location, which is [`lock_path`].
- `fn at`: A lock on an explicit path, for callers that work outside the default store, such as tests and an alternate `$HOME`.
- `fn acquire`: -
- `fn release`: -

## `crates/proot-distro-core/src/login.rs`  84 lines

```
//! Login management for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `struct LoginOptions`: -
- `struct LoginManager`: Login manager
- `fn new`: -
- `fn login`: -
- `fn execute`: -

## `crates/proot-distro-core/src/message.rs`  81 lines

```
//! Message system for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `enum MessageLevel`: -
- `struct Message`: -
- `fn new`: -
- `fn with_metadata`: -
- `trait MessageHandler`: Message handler
- `struct DefaultMessageHandler`: Default message handler

## `crates/proot-distro-core/src/names.rs`  398 lines

```
//! Name validation and management
```

Public items - first doc line, `-` when undocumented:

- `struct NameValidator`: Name validator for containers and images
- `fn new`: -
- `fn validate_container_name`: -
- `fn validate_image_name`: -
- `fn validate_network_name`: -
- `fn validate_volume_name`: -
- `fn store_key`: Directory component that stores an image or container reference
- `fn escape_name`: Escape control and ANSI non-printable bytes for safe terminal output. See issue #05-3.
- `fn is_local_source`: True when `source` is a path on disk rather than an image reference
- `fn is_url_source`: True when `source` is an `http://` or `https://` URL. This is the reference `_is_url` test.
- `fn derive_local_name`: Derive a container name from an archive filename or URL path
- `fn derive_alias`: Derive a short local alias from an image reference

## `crates/proot-distro-core/src/network.rs`  349 lines

```
//! Network module - Stub for future implementation
```

Public items - first doc line, `-` when undocumented:

- `struct NetworkConfig`: -
- `struct NetworkInfo`: -
- `struct NetworkManager`: Network manager
- `fn new`: -
- `fn create`: -
- `fn remove`: -
- `fn list`: -
- `fn inspect`: -
- `fn connect`: Attach `container` to `network`
- `fn disconnect`: Detach `container` from `network`, restoring the default resolver.
- `fn ensure_resolv_conf`: Ensure the guest has a valid, non-empty `/etc/resolv.conf`
- `fn apply_resolv_conf`: Write the guest's `/etc/resolv.conf`

## `crates/proot-distro-core/src/paths.rs`  396 lines

```
//! Path management for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `fn temp_base`: Resolve a writable temp base directory
- `fn resolve_home`: -
- `struct ContainerPaths`: Container paths
- `fn new`: -
- `fn container_rootfs`: -
- `fn container_config`: -
- `fn container_state`: -
- `fn container_logs`: -
- `fn image_path`: -
- `fn ensure_dirs`: -
- `struct PathResolver`: Path resolver for guest/host path translation
- `fn new`: -
- `fn guest_to_host`: -
- `fn host_to_guest`: -
- `const DEFAULT_PREFIX`: `/data/data/com.termux/files/usr`, the documented default.
- `const DEFAULT_PACKAGE`: `com.termux`, the documented default package name.
- `fn resolve_prefix`: Resolve the prefix from explicit values. This split makes the order testable without a change to this process's environment.
- `fn prefix`: The Termux prefix: `TERMUX__PREFIX`, then `PREFIX`, then the default.
- `fn home`: The Termux home: `TERMUX__HOME`, then `HOME`
- `fn resolve_home`: -
- `fn runtime_dir`: `XDG_RUNTIME_DIR`, else `$HOME/.local/run`, else the prefix's tmp directory
- `fn resolve_runtime_dir`: -
- `fn config_dir`: `XDG_CONFIG_HOME`, else `$HOME/.config`.
- `fn resolve_config_dir`: -
- `fn state_dir`: `XDG_STATE_HOME`, else `$HOME/.local/state`
- `fn resolve_state_dir`: -
- `fn package_name`: The Android package name of the host app: `TERMUX_APP__PACKAGE_NAME`, else `TERMUX__PACKAGE_NAME`, else the documented default.
- `fn prefix_for_package`: The prefix for the host app named by [`package_name`], used when the prefix is not exported at all.

## `crates/proot-distro-core/src/progress.rs`  140 lines

```
//! Progress tracking for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `struct ProgressTracker`: Progress tracker
- `fn new`: -
- `fn start`: -
- `fn update`: -
- `fn finish`: -
- `fn get`: -
- `fn list`: -
- `struct ProgressInfo`: -
- `fn progress_percent`: -
- `fn elapsed`: -
- `enum ProgressStatus`: -
- `struct ProgressBar`: Progress bar for CLI
- `fn new`: -
- `fn increment`: -
- `fn finish`: -
- `fn draw`: -

## `crates/proot-distro-core/src/refcount.rs`  471 lines

```
//! Reference-counted eviction for the store (WORKLIST C6).
//!
//! A store unit is deleted exactly when nothing references it: a unit that is
//! still referenced is never deleted, and a unit whose references reach zero is
//! always deleted. The count is **derived**, not stored — it is the size of the
//! set of live references that name the unit, read from the store's own
//! reference source. The reference does the same: `clear-cache --orphan` builds
//! its keep-set from the manifest cache and the build index
//! (`commands/clear_cache.py:_referenced_blob_names`) rather than trusting a
//! counter. A stored count is a second bookkeeping that can disagree with the
//! store it describes, and a count that disagrees deletes live data.
//!
//! ## What was deliberately not built (R3)
//!
//! Leases and the label-graph GC are **out of scope**: the second dissection
//! round puts them out of scope for a single-writer store. What survives is the
//! count. There is no expiry, no renewal, no lease table — a reference is live
//! for exactly as long as its holder is.
//!
//! ## Units today, and how blobs join later
//!
//! The only store unit with a live reference source today is the **staging
//! tree** an install builds a container in before it owns the container's name
//! (`staging_units`). A staging tree is referenced by the live install holding
//! its sidecar `<tree>.lock`; nothing references it once that lock is absent or
//! free, which is the same liveness signal the install path already reads.
//!
//! A content blob (WORKLIST C1) joins by adding one scanner that yields `Unit`s
//! whose `refs` come from the manifest cache and the build index — the reference's
//! two sources. [`evict`] does not change: it deletes iff `refs` is empty.
//!
//! Two things are deliberately **not** units:
//!   * a committed container's rootfs is a *root*, not a refcounted unit — the
//!     operator's data is removed by `remove`, never by an eviction sweep;
//!   * session artifacts already have one accounting (`session::active_sessions`
//!     plus `clear_container_sessions`, A2/A9). A second one here would be the
//!     two-renderings defect that A2 exists to close.
```

Public items - first doc line, `-` when undocumented:

- `const REPLACED_PREFIX`: Prefix of the directory a `--force` reinstall moves the superseded container aside to while it swaps the replacement in
- `struct Unit`: -
- `enum Eviction`: -
- `fn message`: The line a caller prints for a refusal, or `None` when nothing needs saying
- `fn key`: -
- `fn refusal_message`: The exact refusal line for a unit whose reference count is non-zero
- `fn evict`: Evict `unit` iff nothing references it
- `fn sweep`: Evict every unit whose references reached zero, in order.
- `fn sidecar_lock_path`: `<tree>.lock`, the sidecar whose `flock` state is a staging tree's liveness.
- `fn staging_units`: The store units owned by the container store, each with its reference set

## `crates/proot-distro-core/src/runtime.rs`  564 lines

```
//! Unified Container Runtime Backend Abstraction
//!
//! Provides a polymorphic execution interface for container operations via
//! PRoot user-space emulation or User-Mode Linux (UML) kernel backends.
```

Public items - first doc line, `-` when undocumented:

- `enum RuntimeKind`: -
- `struct RuntimeStatus`: -
- `trait RuntimeBackend`: -
- `struct ProotBackend`: PRoot user-space ptrace chroot backend
- `fn new`: Create new PRoot backend with uninitialized engine
- `fn with_engine`: Create PRoot backend reusing an existing engine holder
- `fn parse_uml_exec_exit_code`: Parses command exit code from UML console log output
- `struct UmlBackend`: A User-Mode Linux daemon client runtime backend, where UML means User-Mode Linux.
- `fn new`: Create new UML backend using environment or standard fallback paths
- `fn with_socket`: Create UML backend targeting explicit socket path
- `fn socket_path`: Current socket path
- `fn send_command`: Send one raw command line and return daemon response
- `fn request_with_retry`: Send command with automatic rate-limit backoff retry
- `fn protocol`: Perform runtime protocol handshake query

## `crates/proot-distro-core/src/security.rs`  208 lines

```
//! Security hardening for PRoot containers (re-export from proot-compat)
//!
//! This module re-exports security hardening types from proot-compat
//! to provide a unified API for container security hardening.
```

Public items - first doc line, `-` when undocumented:

- `struct DistroSecurityProfile`: -
- `struct NetworkSecurityPolicy`: -
- `enum DnsPolicy`: -
- `struct FilesystemSecurityPolicy`: -
- `struct DistroSecurityManager`: A distro security manager that extends the base SecurityHardeningManager.
- `fn new`: -
- `fn register_profile`: -
- `fn get_profile`: -
- `fn apply_profile`: -
- `fn assert_seccomp_profile_supported`: Refuse a container whose metadata asks for a seccomp profile. See issue #60.

## `crates/proot-distro-core/src/session.rs`  668 lines

```
//! Session management for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `struct SessionRecord`: -
- `fn sessions_dir`: Directory storing runtime session files.
- `fn session_file_path`: Session file path for a given PID.
- `fn session_socket_path`: The rendezvous socket that a detached session's supervisor listens on. See PDN#57.
- `fn session_log_path`: The transcript that the detached supervisor appends the guest's output to. See PDN#57.
- `fn process_state_and_start_time`: `(state, start-time)` from `/proc/<pid>/stat`, or `None` when the process is gone
- `fn process_start_time`: The start time of a live process, for the record that will describe it.
- `fn process_is_alive`: Liveness of a recorded session: the process exists, its kernel start time is the recorded one, and it is not a zombie
- `fn signal_target`: The pid to signal for a session that the registry calls live. Use `None` when the record's pid is no longer the process that registered it.
- `fn clear_session_files`: Remove every artifact of `pid`'s session: the record, the rendezvous socket and the transcript
- `fn clear_container_sessions`: Reclaim a container's session artifacts once it is killed or removed (A9)
- `fn wait_and_clear`: Wait, with a bound, for `pid` to be gone. Identify the process by `start_time`. Then remove its artifacts.
- `fn reap_stale_rendezvous`: Remove rendezvous files whose supervisor is gone. See PDN#57.
- `fn register_session`: Register a new running session, taking an inherited exclusive lock.
- `struct SessionGuard`: -
- `fn active_sessions`: Registry liveness. While some process holds a record's inherited `LOCK_EX`, the session is live. The moment that lock is free, it is dead, whatev
- `fn kill_session`: Send a signal to a session process.
- `struct SessionManager`: Session manager
- `fn new`: -
- `fn create`: -
- `fn get`: -
- `fn list`: -
- `fn list_all`: -
- `fn list_all_full`: -
- `fn end`: -
- `struct Session`: -
- `enum SessionStatus`: -

## `crates/proot-distro-core/src/shm.rs`  210 lines

```
//! Shared memory management for PRoot Distro
```

Public items - first doc line, `-` when undocumented:

- `struct ShmManager`: Shared memory manager
- `fn new`: -
- `fn create`: -
- `fn open`: -
- `fn remove`: -
- `fn list`: -
- `struct ShmMapping`: Shared memory mapping
- `fn read`: -
- `fn write`: -
- `fn as_slice`: -
- `fn as_mut_slice`: -
- `fn size`: -
- `fn flush`: -
- `struct ShmHandle`: Shared memory handle
- `fn name`: -
- `fn read`: -
- `fn write`: -
- `fn size`: -
- `fn as_slice`: -

## `crates/proot-distro-core/src/signature.rs`  476 lines

```
//! Image signature verification for PRoot Distro
//!
//! Provides signature verification using cosign, notary, and simple verification.
//! Supports keyless signing, keypair signing, and notary v1/v2.
```

Public items - first doc line, `-` when undocumented:

- `struct SignatureConfig`: -
- `struct VerificationResult`: -
- `enum SignatureType`: -
- `struct TlogEntry`: -
- `struct ImageSignature`: -
- `struct SignatureVerifier`: Signature verifier
- `fn new`: -
- `fn verify_image`: Verify an image signature
- `fn add_trusted_key`: Add a trusted public key
- `fn add_trusted_certificate`: Add a trusted certificate
- `fn extract_signature_from_annotations`: Extract the signature from image annotations. Cosign stores signatures in annotations.
- `fn verify_keyless_signature`: Verify an image with cosign keyless signing. This is a placeholder for a future implementation.

## `crates/proot-distro-core/src/storage.rs`  317 lines

```
//! Storage/Volume module - Stub for future implementation
```

Public items - first doc line, `-` when undocumented:

- `struct VolumeConfig`: -
- `struct VolumeInfo`: -
- `struct StorageManager`: Storage manager
- `fn new`: -
- `fn tmp_manager`: -
- `fn mountpoint`: -
- `fn create_volume`: -
- `fn remove_volume`: -
- `fn list_volumes`: -
- `fn inspect_volume`: -
- `struct FileId`: -
- `fn file_id`: Stat `path` *without* following a final symlink and report its identity
- `fn rename_if_unchanged`: Rename `tmp` onto `dest`, but only if `tmp` still names the inode `expected`
- `fn atomic_write`: Write `bytes` to `path` atomically: write a sibling temp, then rename it into place only while its identity is unchanged

## `crates/proot-distro-core/src/sysdata.rs`  745 lines

```
//! Fake `/proc` entries for containers.
//!
//! Android restricts or outright blocks a number of `/proc` files, so anything
//! inside a container that reads them (`top`, `htop`, `free`, `uptime`,
//! `nproc`) sees a permission error or nothing at all. proot-distro works
//! around this by writing static stand-ins into `containers/<name>/sysdata/`
//! and bind-mounting them over the real paths; this module does the same.
//!
//! Two rules matter:
//!
//! * The stubs live *beside* the rootfs, never inside it. The rootfs `/proc` is
//!   masked by `--bind=/proc`, so a file written into the image is invisible —
//!   which is what the previous implementation of this module did, and why none
//!   of it ever had an effect.
//! * A stub is bound only when the host's own file cannot be read, so a device
//!   that does expose the real file keeps the real file.
//!
//! The content is static except for `stat` and `uptime`, which are regenerated
//! on every call so their `cpuN` lines agree with the CPU count of the device
//! the container is started on.
```

Public items - first doc line, `-` when undocumented:

- `fn system_uptime`: The system uptime in seconds, from CLOCK_BOOTTIME.
- `fn boot_time_seconds`: Boot time in seconds since Unix epoch, matching `system_uptime`.
- `fn fake_vmstat`: -
- `fn sysdata_dir`: Directory holding the stubs: a sibling of the rootfs, so it is removed together with the container.
- `fn host_cpu_ids`: Online CPU ids, mirroring what the guest's own `/proc/cpuinfo` will list.
- `fn fake_stat`: `/proc/stat` with one `cpuN` line per online CPU
- `fn fake_stat_with_uptime`: -
- `fn fake_uptime`: `/proc/uptime`, the second field being total idle ticks over all CPUs so it stays consistent with the idle column of `cpu` in `fake_stat`.
- `fn fake_uptime_with_uptime`: -
- `fn ensure`: Write the stubs under `<container_dir>/sysdata/`
- `fn bindings`: `--bind=<src>:<dst>` arguments for the stubs whose host counterpart cannot be read

## `crates/proot-distro-core/src/untar.rs`  642 lines

```
//! Archive extraction that survives Android.
//!
//! `tar::Archive::unpack` is the obvious call and the wrong one here. This
//! device's SELinux policy refuses `link(2)` **outright** — verified: `ln` on
//! two names in the same app-owned directory on f2fs returns
//! `Permission denied`. Any image whose layers contain hard links therefore
//! aborts the whole install:
//!
//! ```text
//! Error: failed to unpack `.../rootfs/usr/bin/perl5.40.1`
//! Caused by: Permission denied (os error 13) when hard linking
//!            .../usr/bin/perl to .../usr/bin/perl5.40.1
//! ```
//!
//! Alpine installs fine (busybox applets are symlinks), Debian does not
//! (`perl` -> `perl5.40.1` is a hard link), which is exactly the difference the
//! user hit.
//!
//! So hard links are materialised as independent copies: identical bytes, which
//! is all the guest needs, at the cost of the deduplication the link provided.
//! Special files (device nodes, FIFOs) cannot be created without root and are
//! skipped rather than failing the extraction.
//!
//! Ownership is not restored — an unprivileged Android app cannot `chown`.
//! Modes are applied, after the content is written so a read-only bit cannot
//! block the write.
//!
//! OCI whiteout markers are obeyed. `layer_diff.rs` writes them when it diffs
//! two trees, and until this side read them the loop was open: a layer that
//! deleted `/etc/old.conf` produced `.wh.old.conf`, extraction created that
//! marker as an ordinary empty file, and the deleted file was still there
//! afterwards. See `remove_whiteouted`.
//!
//! The *opaque directory* marker (`.wh..wh..opq`) is deliberately not handled:
//! nothing in this project emits it, and only a foreign layer could carry one.
//! That arrives with the layer import in v0.2.0, and it needs a test that shows
//! the lower layer's contents disappearing — which cannot be written against a
//! code path that does not exist yet.
```

Public items - first doc line, `-` when undocumented:

- `static DIR_CALLS`: -
- `fn extract`: Unpack a tar stream into `dest`, creating it if needed.
- `fn extract_with_strip`: Unpack a tar stream into `dest`, dropping the first `strip` path components of every member
- `fn extract_entries`: Unpack an already-open archive, for callers that need to inspect it first.
- `fn extract_entries_stripped`: `extract_entries` with a strip count. See [`extract_with_strip`].

## `crates/proot-distro-core/tests/atomic_identity.rs`  124 lines

```
//! B11: an atomic state write refuses (`ESTALE`) rather than clobbering when the
//! temp file is replaced or removed between the write and the rename.
//!
//! The swap is PLANTED, never raced: each test builds the temp file itself,
//! replaces or removes it, then drives the identity gate (`rename_if_unchanged`)
//! directly — the same step `atomic_write` performs around its rename. No test
//! wins a timing window; the condition is deterministic.
```

## `crates/proot-distro-core/tests/blob_store_dedup.rs`  389 lines

```
//! Content-addressed layer store: a shared base layer is stored ONCE, and a
//! container can be materialised from the store rather than from a freshly
//! unpacked archive.
//!
//! The acceptance is a number, not a claim. Two local OCI archives are built
//! here that share one base layer (`img1 = [base, appA]`, `img2 = [base, appB]`);
//! after both are installed the store holds **three** blobs, not four, and its
//! byte total is `base + appA + appB` - not the `2*base + appA + appB` a store
//! without content addressing would hold. A `du`-style line is printed for the
//! throwaway store root, and the count/byte assertions fail if the shared blob
//! is ever duplicated.
//!
//! **Negative control.** The pre-existing suites (`store_integrity`,
//! `atomic_identity`) are the base-tree controls and must stay green; this file
//! adds only the new store behaviour. Nothing here touches the operator's real
//! `~/.podnext`: the store root is redirected through `PN_TEST_ROOT` (the same
//! resolver `paths::resolve_home` honours), and every store in this file lives
//! under a `tempdir`.
```

## `crates/proot-distro-core/tests/deep_fs_recursion.rs`  140 lines

*No `//!` module doc. The map records this gap.*

## `crates/proot-distro-core/tests/single_writer_races.rs`  522 lines

```
//! A29: race the four mechanisms `research/RACE_SURFACES.md` (section 4c) says are
//! single-writer **by construction** but which the tree has only ever proven
//! sequentially. The value here is evidence, not new code: turn "should hold"
//! into "was raced and held". No mechanism is added.
//!
//! The four, each with the citation the item already established:
//!
//! 1. `blobstore::put_reader` two-phase ingest - the temp name carries `pid` and
//!    a counter (`blobstore.rs:161`), publication is `rename_if_unchanged` with an
//!    identity check (`:196`). Two processes ingest the SAME digest at once.
//! 2. `refcount::sweep` vs a live install - both take the same sidecar `flock`
//!    (`refcount.rs:168-170`). A sweep races an install that holds it.
//! 3. store `list` during `install` - staging trees are named `.tmp-install-*`
//!    (`container.rs:899`). `scan_store` runs under install load.
//! 4. session register vs attach - registration takes `LOCK_EX`
//!    (`session.rs:251`) and liveness is the inherited descriptor (C8). Attach
//!    (the registry scan) races registration.
//!
//! Every root is redirected: the blob store and store root are explicit paths
//! under a `tempdir`, the session registry is a private `RUNTIME_DIR`. Nothing
//! here touches the operator's real `~/.podnext` (10 containers) or its session
//! registry. Every wait is bounded; no test can hang the suite.
```

## `crates/proot-distro-core/tests/store_fifo_is_metadata.rs`  116 lines

```
//! Store-integrity acceptance tests for WORKLIST S3: a FIFO at a container's
//! `metadata.json` is *metadata* - classified by type and refused by name - and
//! never a device node the store opens.
//!
//! A1 bounded the symptom: the scan classified the path with `O_NONBLOCK` so it
//! could not block, and then listed the container as a corrupt entry. The
//! campaign's planted FIFO was still a *pseudo-container* in the listing. S3 is
//! the structural form: the store reads metadata only from a regular file, and
//! anything else at that path (FIFO, socket, device, directory, symlink) fails
//! the store operation with an error naming the path and the file type, raised
//! before any read is attempted.
//!
//! The bound is the assertion, not a hope: a read of a FIFO blocks in `openat`
//! until a writer appears, so a test that returns inside its budget has proved
//! nothing was opened.
//!
//! The assertion runs against the *whole* store operation, not a helper, so the
//! base tree is a real control: there the scan returns `Ok` (the FIFO container
//! listed as corrupt) and the `expect_err` below fails, which is what this test
//! is for. Nothing here touches the real `~/.podnext`; the store lives in a
//! `tempdir` that unlinks the planted FIFO on drop even when a test fails.
```

## `crates/proot-distro-core/tests/store_integrity.rs`  435 lines

```
//! Store-integrity acceptance tests for WORKLIST A1, A3a and A4.
//!
//! Every test bounds its own wait: a check that can hang is not a check, so the
//! tests assert an *elapsed* budget rather than trusting the call to return.
//!
//! The store is built under a temporary directory and reached through
//! `PN_TEST_ROOT` (the same override `paths::resolve_home` honours), so no test
//! touches the real `~/.podnext`.
```

# tests


## `tests/daemon_client_e2e.rs`  48 lines

```
//! Test client for podnext-daemon
```

## `tests/daemon_integration_test.rs`  375 lines

```
//! Daemon Integration Tests
//! Tests full podnext-daemon lifecycle via Unix socket IPC
```

## `tests/integration/common.rs`  664 lines

```
//! Common test utilities for integration tests
```

Public items - first doc line, `-` when undocumented:

- `struct TestEnvironment`: Test environment that provides access to all PN Next components
- `fn new`: Create a new test environment with all components initialized
- `fn create_service_config`: Create a service configuration using supervisor's create_service method
- `fn start_service`: Start a service
- `fn stop_service`: Stop a service
- `fn remove_service`: Remove a service
- `fn list_services`: List all services
- `fn get_service_status`: Get service status
- `fn write_service_toml`: Write service TOML config
- `fn load_service_from_toml`: Load service from TOML
- `fn write_socket_config`: Write socket config
- `fn register_socket_activation`: Register socket activation
- `fn connect_to_socket`: Connect to the socket. This triggers socket activation.
- `fn enable_boot_autostart`: Enable boot auto-start
- `fn get_boot_script_path`: Get boot script path
- `fn acquire_wake_lock`: Acquire wake lock
- `fn release_wake_lock`: Release wake lock
- `fn is_wake_lock_held`: Check if wake lock is held
- `fn send_dbus_message`: Send D-Bus message
- `fn write_log_entry`: Write log entry
- `fn flush_logs`: Flush logs
- `fn query_logs`: Query logs
- `fn get_log_stats`: Get log stats
- `fn set_watchdog_policy`: Set watchdog policy
- `fn get_watchdog_state`: Get watchdog state
- `fn set_resource_quota`: Set resource quota
- `fn get_resource_quotas`: Get resource quotas
- `fn collect_usage`: Collect resource usage
- `fn verify_build`: Verify build
- `fn run_unit_tests`: Run unit tests
- `fn test_gateway_failsafe`: Test gateway fail-safe
- `fn verify_docs`: Verify documentation exists
- `fn create_test_network`: Create a test network
- `fn connect_container_to_network`: Connect container to network
- `fn disconnect_container_from_network`: Disconnect container from network
- `fn create_test_volume`: Create a test volume
- `fn get_volume`: Get volume
- `fn remove_volume`: Remove volume
- `fn verify_volume_persistence`: Verify volume persistence. The data survives a container restart.
- `fn cleanup`: Cleanup test environment

## `tests/integration/v2_criteria.rs`  616 lines

```
//! v2.0 Success Criteria Integration Tests
//! Validates all v2.0 Service Orchestration Platform requirements
```

---

Harvested items: 1874

Files with NO module doc: 3

- `crates/podnext/examples/test_daemon_client.rs`
- `crates/podnext/tests/automation_contract.rs`
- `crates/proot-distro-core/tests/deep_fs_recursion.rs`
