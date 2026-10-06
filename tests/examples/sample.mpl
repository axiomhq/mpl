// look at a tenth of the series to get a feel for a large metric
test:http_requests_total
| sample 0.1
| align to 5m using prom::rate
| group by method using sum
