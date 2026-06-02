// Compare two tags for equality to isolate service self-calls: series where
// the calling service (client) and the called service (server) are identical.
// Flip to `!=` to see only cross-service RPCs instead.
service_mesh:rpc_call_duration_ms
| where client == server
