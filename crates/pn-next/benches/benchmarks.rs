//! Performance benchmarks for pn-next

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use pn_next::{ContainerConfig, Resources, RuntimeManager, SecurityProfile};
use std::time::Duration;

fn bench_sandbox_creation(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let runtime = rt.block_on(RuntimeManager::new()).unwrap();

    let mut group = c.benchmark_group("sandbox_creation");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(10);

    group.bench_function("create_sandbox", |b| {
        b.to_async(&rt).iter(|| async {
            let sandbox = runtime
                .create_sandbox(black_box(pn_next::SandboxConfig {
                    hostname: "bench-sandbox".to_string(),
                    network: "bridge".to_string(),
                    ..Default::default()
                }))
                .await
                .unwrap();
            black_box(sandbox);
        });
    });

    group.finish();
}

fn bench_container_creation(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let runtime = rt.block_on(RuntimeManager::new()).unwrap();

    let mut group = c.benchmark_group("container_creation");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(10);

    group.bench_function("create_container", |b| {
        b.to_async(&rt).iter(|| async {
            let sandbox = runtime
                .create_sandbox(pn_next::SandboxConfig {
                    hostname: "bench-sandbox".to_string(),
                    network: "bridge".to_string(),
                    ..Default::default()
                })
                .await
                .unwrap();

            let container = runtime
                .create_container(ContainerConfig {
                    sandbox_id: sandbox.id,
                    image: "docker.io/library/alpine:latest".to_string(),
                    config: Some(ContainerConfigInner {
                        hostname: "bench-container".to_string(),
                        working_dir: "/root".to_string(),
                        resources: Some(Resources {
                            memory_limit: "512M".to_string(),
                            cpu_limit: "1.0".to_string(),
                            pids_limit: 50,
                        }),
                        security_profile: SecurityProfile::Default,
                        ..Default::default()
                    }),
                })
                .await
                .unwrap();
            black_box(container);
        });
    });

    group.finish();
}

fn bench_exec_throughput(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let runtime = rt.block_on(RuntimeManager::new()).unwrap();

    let sandbox = rt
        .block_on(runtime.create_sandbox(pn_next::SandboxConfig {
            hostname: "bench-sandbox".to_string(),
            network: "bridge".to_string(),
            ..Default::default()
        }))
        .unwrap();

    let container = rt
        .block_on(runtime.create_container(ContainerConfig {
            sandbox_id: sandbox.id,
            image: "docker.io/library/alpine:latest".to_string(),
            config: Some(ContainerConfigInner {
                hostname: "bench-container".to_string(),
                working_dir: "/root".to_string(),
                resources: Some(Resources {
                    memory_limit: "512M".to_string(),
                    cpu_limit: "1.0".to_string(),
                    pids_limit: 50,
                }),
                security_profile: SecurityProfile::Default,
                ..Default::default()
            }),
        }))
        .unwrap();

    rt.block_on(runtime.start_container(&container.id)).unwrap();

    let mut group = c.benchmark_group("exec_throughput");
    group.measurement_time(Duration::from_secs(30));
    group.sample_size(20);

    for size in [1, 10, 100].iter() {
        group.throughput(Throughput::Elements(*size));
        group.bench_with_input(BenchmarkId::new("exec_batch", size), size, |b, &size| {
            b.to_async(&rt).iter(|| async {
                let mut handles = Vec::new();
                for _ in 0..size {
                    let runtime = runtime.clone();
                    let container_id = container.id.clone();
                    handles.push(tokio::spawn(async move {
                        runtime
                            .exec(
                                container_id,
                                pn_next::ExecConfig {
                                    command: vec!["echo".to_string(), "hello".to_string()],
                                    tty: false,
                                    ..Default::default()
                                },
                            )
                            .await
                    }));
                }
                for handle in handles {
                    let _ = handle.await;
                }
            });
        });
    }

    group.finish();
}

fn bench_guardrails_evaluation(c: &mut Criterion) {
    use pn_gateway::{GuardrailsConfig, GuardrailsEngine, ResourceRequest};

    let rt = tokio::runtime::Runtime::new().unwrap();
    let guardrails = GuardrailsEngine::new(GuardrailsConfig::default()).unwrap();

    let mut group = c.benchmark_group("guardrails_evaluation");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(50);

    group.bench_function("evaluate_request", |b| {
        b.to_async(&rt).iter(|| async {
            let request = ResourceRequest {
                container_id: "test-container".to_string(),
                memory_bytes: 1024 * 1024 * 1024, // 1GB
                cpu_cores: 1.0,
                pids: 50,
            };
            let result = guardrails.evaluate(black_box(request)).await.unwrap();
            black_box(result);
        });
    });

    group.finish();
}

fn bench_fail_safe_circuit_breaker(c: &mut Criterion) {
    use pn_gateway::{FailSafeConfig, FailSafeManager};

    let rt = tokio::runtime::Runtime::new().unwrap();
    let fail_safe = FailSafeManager::new(FailSafeConfig::default()).unwrap();

    let mut group = c.benchmark_group("fail_safe_circuit_breaker");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(50);

    group.bench_function("execute_with_circuit_breaker", |b| {
        b.to_async(&rt).iter(|| async {
            let result = fail_safe
                .execute(black_box("test-operation"), || async {
                    Ok::<_, anyhow::Error>("success".to_string())
                })
                .await;
            black_box(result);
        });
    });

    group.finish();
}

fn bench_seccomp_filter_generation(c: &mut Criterion) {
    use proot_compat::security::{SeccompProfile, SecurityHardeningManager};

    let mut group = c.benchmark_group("seccomp_filter_generation");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    group.bench_function("generate_default_profile", |b| {
        b.iter(|| {
            let profile = SecurityHardeningManager::generate_default_seccomp_profile();
            black_box(profile);
        });
    });

    group.bench_function("generate_restricted_profile", |b| {
        b.iter(|| {
            let profile = SecurityHardeningManager::generate_restricted_seccomp_profile();
            black_box(profile);
        });
    });

    group.finish();
}

fn bench_path_translation(c: &mut Criterion) {
    use proot_compat::path_translation::{PathTranslationConfig, PathTranslator};
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let config = PathTranslationConfig {
        host_prefix: temp_dir.path().to_path_buf(),
        guest_prefix: PathBuf::from("/guest"),
        bind_mounts: vec![],
        no_symlinks: false,
    };

    let translator = PathTranslator::new(config).unwrap();

    let paths = vec![
        Path::new("/simple/path"),
        Path::new("/deep/nested/path/with/many/components"),
        Path::new("/path/with/symlink"),
    ];

    let mut group = c.benchmark_group("path_translation");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(100);

    group.bench_function("translate_to_host", |b| {
        b.iter(|| {
            for path in &paths {
                let _ = translator.translate_to_host(black_box(*path));
            }
        });
    });

    group.bench_function("translate_to_guest", |b| {
        b.iter(|| {
            for path in &paths {
                let _ = translator.translate_to_guest(black_box(*path));
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_sandbox_creation,
    bench_container_creation,
    bench_exec_throughput,
    bench_guardrails_evaluation,
    bench_fail_safe_circuit_breaker,
    bench_seccomp_filter_generation,
    bench_path_translation,
);
criterion_main!(benches);
