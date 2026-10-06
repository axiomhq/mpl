// sum(rate(http_requests_total[5m]))
// /
// sum(rate(http_requests_total[5m] offset 1w))

(
    test:http_requests_total
    | align to 5m using prom::rate
    | group using sum,
    test:http_requests_total
    | offset -1w
    | align to 5m using prom::rate
    | group using sum
)
| compute week_over_week using /
