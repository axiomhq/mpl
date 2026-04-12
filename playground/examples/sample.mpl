  test:http_requests_total
  | sample 11
  | where code >= 500
  | group using sum

