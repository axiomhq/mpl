test:axiom_http_request_duration_seconds_bucket
| where code == #/[123]../
| bucket by method, path to 5m using interpolate_delta_histogram(rate, 0.90, 0.99)
