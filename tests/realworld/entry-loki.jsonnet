local loki = import 'loki/loki.libsonnet';

loki {
  _config+:: {
    namespace: 'loki',
    cluster: 'loki-bench',
    storage_backend: 's3',
    s3_address: 's3.example.com',
    s3_bucket_name: 'loki-bench',
    s3_access_key: 'AKIA',
    s3_secret_access_key: 'SECRET',
    boltdb_shipper_shared_store: 's3',

    using_boltdb_shipper: false,
    using_tsdb_shipper: true,
    use_index_gateway: true,

    loki+: {
      schema_config+: {
        configs: [{
          from: '2024-01-01',
          store: 'tsdb',
          object_store: 's3',
          schema: 'v13',
          index: { prefix: 'loki_index_', period: '24h' },
        }],
      },
    },
  },
}
