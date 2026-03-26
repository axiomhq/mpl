`test-with-minus.com`:axiomdb_transport_ingest_pressure as snot
| where (time_window is string and time_window == "1m") or (time_window is int and time_window == 60)
| align to 7d using avg
| as cake
