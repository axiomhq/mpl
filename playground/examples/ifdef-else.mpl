param $container: Option<string>;

// When `$container` is supplied, filter to that container; otherwise fall
// back to the "default" container. The playground doesn't bind optional
// params, so the else branch is what you'll see executed below.
test:http_requests_total
| ifdef($container) { where container == $container } else { where container == "default" }
| align to 5m using avg
