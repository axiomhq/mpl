// sum(
//   rate(
//       http_request_duration_seconds_count{
//           container=~"ingest-pod|api-pod|login-pod|integrations-pod",
//           path=~".*(elastic/_bulk|ingest|(?:v1/(traces|logs|metrics))).*",
//           code!~"[1234].."
//       }[5m]
//   )
// )
// /
// sum(
//   rate(
//       http_request_duration_seconds_count{
//           container=~"ingest-pod|api-pod|login-pod|integrations-pod",
//           path=~".*(elastic/_bulk|ingest|(?:v1/(traces|logs|metrics))).*"
//       }[5m]
//   )
// )


(
    test:http_request_duration_seconds_count
    | where container == #/ingest-pod|api-pod|login-pod|integrations-pod/
    | where path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
    | where code >= 500 // note since things go through prom we currently store it as string but this is how we'd want this query to IO_WORKER
    | align to 5m using prom::rate
    | group using sum,
    test:http_request_duration_seconds_count
    | where container == #/ingest-pod|api-pod|login-pod|integrations-pod/
    | where path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
    | align to 5m using prom::rate
    | group using sum
)
| compute error_rate using /
