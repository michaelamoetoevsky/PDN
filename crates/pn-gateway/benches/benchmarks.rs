//! Performance benchmarks for pn-gateway

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use pn_gateway::{GuardrailsEngine, GuardrailsConfig, ResourceRequest, FailSafeManager, FailSafeConfig};
use std::time::Duration;

fn bench_guardrails_evaluation(c: &mut Criterion) {
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

fn bench_guardrails_batch_evaluation(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let guardrails = GuardrailsEngine::new(GuardrailsConfig::default()).unwrap();

    let mut group = c.benchmark_group("guardrails_batch_evaluation");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(20);

    for batch_size in [1, 10, 50, 100].iter() {
        group.throughput(Throughput::Elements(*batch_size));
        group.bench_with_input(BenchmarkId::new("batch_evaluate", batch_size), batch_size, |b, &size| {
            b.to_async(&rt).iter(|| async {
                let mut handles = Vec::new();
                for i in 0..size {
                    let guardrails = guardrails.clone();
                    handles.push(tokio::spawn(async move {
                        let request = ResourceRequest {
                            container_id: format!("container-{}", i),
                            memory_bytes: 512 * 1024 * 1024, // 512MB
                            cpu_cores: 0.5,
                            pids: 25,
                        };
                        guardrails.evaluate(request).await
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

fn bench_fail_safe_circuit_breaker(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let fail_safe = FailSafeManager::new(FailSafeConfig::default()).unwrap();

    let mut group = c.benchmark_group("fail_safe_circuit_breaker");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(50);

    group.bench_function("execute_success", |b| {
        b.to_async(&rt).iter(|| async {
            let result = fail_safe.execute(black_box("test-op-success"), || async {
                Ok::<_, anyhow::Error>("success".to_string())
            }).await;
            black_box(result);
        });
    });

    group.bench_function("execute_failure", |b| {
        b.to_async(&rt).iter(|| async {
            let result = fail_safe.execute(black_box("test-op-failure"), || async {
                Err::<String, _>(anyhow::anyhow!("simulated failure"))
            }).await;
            black_box(result);
        });
    });

    group.finish();
}

fn bench_fail_safe_retry_policy(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let fail_safe = FailSafeManager::new(FailSafeConfig {
        retry_policy: pn_gateway::RetryPolicyConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            retryable_errors: vec!["timeout".to_string(), "unavailable".to_string()],
        },
        ..Default::default()
    }).unwrap();

    let mut group = c.benchmark_group("fail_safe_retry_policy");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(20);

    group.bench_function("retry_on_failure", |b| {
        b.to_async(&rt).iter(|| async {
            let attempt = std::sync::atomic::AtomicUsize::new(0);
            let result = fail_safe.execute(black_box("retry-test"), || async {
                let current = attempt.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if current < 2 {
                    Err(anyhow::anyhow!("timeout"))
                } else {
                    Ok("success".to_string())
                }
            }).await;
            black_box(result);
        });
    });

    group.finish();
}

fn bench_fail_safe_bulkhead(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let fail_safe = FailSafeManager::new(FailSafeConfig {
        bulkhead: pn_gateway::BulkheadConfig {
            max_concurrent: 10,
            queue_size: 50,
            timeout: Duration::from_secs(5),
        },
        ..Default::default()
    }).unwrap();

    let mut group = c.benchmark_group("fail_safe_bulkhead");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(20);

    for concurrent in [1, 5, 10, 20].iter() {
        group.throughput(Throughput::Elements(*concurrent));
        group.bench_with_input(BenchmarkId::new("bulkhead_concurrent", concurrent), concurrent, |b, &size| {
            b.to_async(&rt).iter(|| async {
                let mut handles = Vec::new();
                for _ in 0..size {
                    let fail_safe = fail_safe.clone();
                    handles.push(tokio::spawn(async move {
                        fail_safe.execute("bulkhead-test", || async {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            Ok("done".to_string())
                        }).await
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

fn bench_health_monitor(c: &mut Criterion) {
    use pn_gateway::{HealthMonitor, HealthConfig, HealthCheck};

    let rt = tokio::runtime::Runtime::new().unwrap();
    let health = HealthMonitor::new(HealthConfig::default()).unwrap();

    let mut group = c.benchmark_group("health_monitor");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(50);

    group.bench_function("register_check", |b| {
        b.iter(|| {
            let check = HealthCheck {
                name: "test-check".to_string(),
                check_type: pn_gateway::HealthCheckType::HTTP {
                    url: "http://localhost:8080/health".to_string(),
                    expected_status: 200,
                },
                interval: Duration::from_secs(30),
                timeout: Duration::from_secs(5),
                healthy_threshold: 2,
                unhealthy_threshold: 3,
            };
            health.register_check(black_box(check)).unwrap();
        });
    });

    group.bench_function("check_health", |b| {
        b.to_async(&rt).iter(|| async {
            let result = health.check_all().await.unwrap();
            black_box(result);
        });
    });

    group.finish();
}

fn bench_metrics_collection(c: &mut Criterion) {
    use pn_gateway::{MetricsCollector, MetricsConfig};

    let rt = tokio::runtime::Runtime::new().unwrap();
    let metrics = MetricsCollector::new(MetricsConfig::default()).unwrap();

    let mut group = c.benchmark_group("metrics_collection");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(50);

    group.bench_function("record_counter", |b| {
        b.iter(|| {
            metrics.increment_counter(black_box("test_counter"), black_box(&[("label", "value")]));
        });
    });

    group.bench_function("record_histogram", |b| {
        b.iter(|| {
            metrics.record_histogram(black_box("test_histogram"), black_box(123.456), black_box(&[("label", "value")]));
        });
    });

    group.bench_function("record_gauge", |b| {
        b.iter(|| {
            metrics.set_gauge(black_box("test_gauge"), black_box(42.0), black_box(&[("label", "value")]));
        });
    });

    group.bench_function("export_metrics", |b| {
        b.to_async(&rt).iter(|| async {
            let result = metrics.export().await.unwrap();
            black_box(result);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_guardrails_evaluation,
    bench_guardrails_batch_evaluation,
    bench_fail_safe_circuit_breaker,
    bench_fail_safe_retry_policy,
    bench_fail_safe_bulkhead,
    bench_health_monitor,
    bench_metrics_collection,
);
criterion_main!(benches);