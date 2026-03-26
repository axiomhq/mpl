`axiom-dev.metrics`:axiom_http_requests_total
| filter path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align to 42s using prom::rate
| group by method, path, code using sum
