( simple:example
  | align using last
  | group by host using sum
  | extend url = "http://${host}"
  | extend scheme = "http",
  simple:example
  | align using last
  | group by host using sum
  | extend url = "http://${host}",
) | compute sum using +
