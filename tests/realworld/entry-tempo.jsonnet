local tempo = import 'github.com/grafana/tempo/operations/jsonnet/microservices/tempo.libsonnet';

tempo {
  _images+:: {
    tempo: 'grafana/tempo:latest',
    tempo_vulture: 'grafana/tempo-vulture:latest',
    tempo_query: 'grafana/tempo-query:latest',
  },

  _config+:: {
    namespace: 'tracing',
    distributor+: {
      receivers: {
        otlp: { protocols: { grpc: { endpoint: '0.0.0.0:4317' } } },
      },
    },
    metrics_generator+: {
      pvc_size: '10Gi',
      pvc_storage_class: 'fast',
      ephemeral_storage_request_size: '10Gi',
      ephemeral_storage_limit_size: '11Gi',
    },
    live_store+: {
      pvc_size: '10Gi',
      pvc_storage_class: 'fast',
    },
    backend_scheduler+: {
      pvc_size: '200Mi',
      pvc_storage_class: 'fast',
    },
    backend: 'gcs',
    bucket: 'tempo-bench',
    kafka_address: 'kafka:9092',
    kafka_topic: 'tempo',
  },
}
