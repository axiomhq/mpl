// sum by (method, path, code) (
//       rate(
//           http_requests_total{
//               path=~".*(elastic/_bulk|ingest|(?:v1/(traces|logs|metrics))).*",
//               code=~"[123].."
//           }[5m]
//       )
//   )

test:http_requests_total
| where path != #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| where code < 400
| align to 5m using prom::rate
| group by method, path, code using sum
