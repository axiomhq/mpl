  test:http_requests_total
  | sample 1.0
  | where code >= 500
  | group using sum
