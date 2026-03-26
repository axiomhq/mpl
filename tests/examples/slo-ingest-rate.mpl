// avg_over_time(
//     ((
//         sum(rate(axiom_http_requests_total{container=~"axiom-ingest-pod|axiom-api-pod|axiom-login-pod|axiom-integrations-pod", path=~".*(elastic/_bulk|ingest|(?:v1/(traces|logs|metrics))).*", code!~"[1234].."}[1h]))
//         /
//         sum(rate(axiom_http_requests_total{container=~"axiom-ingest-pod|axiom-api-pod|axiom-login-pod|axiom-integrations-pod", path=~".*(elastic/_bulk|ingest|(?:v1/(traces|logs|metrics))).*"}[1h]))
//     ) <bool 0.2)[7d:]
// )

(
  `test`:axiom_http_requests_total
  | where container == #/axiom-ingest-pod|axiom-api-pod|axiom-login-pod|axiom-integrations-pod/
  | where path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
  | where code >= 500
  | align to 1h using prom::rate
  | group using sum,

  test:axiom_http_requests_total
  | where container == #/axiom-ingest-pod|axiom-api-pod|axiom-login-pod|axiom-integrations-pod/
  | where path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
  | align to 1h using prom::rate
  | group using sum,
)
| compute error_rate using /
| map is::lt(0.2)
| align to 1h over 7d using avg
