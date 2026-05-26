local mimir = import 'github.com/grafana/mimir/operations/mimir/mimir.libsonnet';

mimir {
  _config+:: {
    namespace: 'mimir',
    cluster: 'mimir-bench',
    external_url: 'http://mimir.example.com',

    storage_backend: 'gcs',
    blocks_storage_bucket_name: 'mimir-blocks',
    ruler_storage_bucket_name: 'mimir-ruler',
    alertmanager_storage_bucket_name: 'mimir-alertmanager',
  },
}
