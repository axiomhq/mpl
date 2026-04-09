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


// this 0.9 is related to the LE bucket stored in the histogram's tags
test:http_request_duration_seconds_bucket
| where code == #/[123]../
| bucket by method, path to 5m using interpolate_delta_histogram(0.90, max, 0.99)
