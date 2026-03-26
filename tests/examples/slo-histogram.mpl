// SLI for p99 latency lower than 400ms
// avg_over_time(
//  (
//    histogram_quantile(
//      0.99,
//      sum by (le) (rate(http_server_request_duration_seconds_bucket[1m]))
//    ) <bool 0.4
//  )[7d:]
// ) * 100


test:http_server_request_duration_seconds_bucket
| bucket to 1m using interpolate_delta_histogram(0.99)
| map is::lt(0.4)
| align to 7d using avg
