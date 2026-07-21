test:http_requests_total
| where tag in ["a", "b", "c", 1, 2.3, false] or not tag in []
