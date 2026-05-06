param $container: Option<string>;

`dev.metrics`:http_requests_total
| ifdef($container) { where container == $container }
| align to 5m using avg
