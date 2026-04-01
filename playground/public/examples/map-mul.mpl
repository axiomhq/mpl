
// http_request_duration_seconds_count{} * 100

test:http_request_duration_seconds_count
| map * 100
