`dev.metrics`:transport_request_duration_seconds_bucket
| where status == #/[123..]/
| where handler == #/^ingest$/
| where le == #/^0\.5/
| map rate
// | align to 30s using avg
| group by handler, le using sum
// | bucket by handler using interpolate_histogram(30s, min, 0.5, max)
