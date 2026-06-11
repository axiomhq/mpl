`dev.metrics`:http_requests_total
| where path == #/.*(elastic\/_bulk|ingest|(?:v1\/(traces|logs|metrics))).*/
| where code == #/[123]../
| align to 5m using prom::rate
| group by method, path, code using sum
