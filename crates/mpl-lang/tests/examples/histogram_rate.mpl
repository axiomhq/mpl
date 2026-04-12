// histogram_quantile(
//   0.90,
//   sum by (method, path, le) (
//       rate(
//          http_request_duration_seconds_bucket{
//              code=~"[123].."
//          }[5m]
//       )
//   )
// )

test:http_request_duration_seconds_bucket
| where code == #/[123]../
| bucket by method, path to 5m using interpolate_cumulative_histogram(rate, 0.90, 0.99)
