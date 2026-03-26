`axiom-dev.metrics`:axiomdb_transport_request_duration_seconds_bucket
| filter status == #/[123..]/
| filter handler == #/^ingest$/
| filter le == #/^0\.5/
| map rate
// | align to 30s using avg
| group by handler, le using sum
// | bucket by handler using interpolate_histogram(30s, min, 0.5, max)
