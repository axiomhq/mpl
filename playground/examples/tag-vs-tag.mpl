// Tag-vs-tag comparison: a tag can be referenced as a value, so the RHS of a
// filter can be another tag instead of a constant. Here we keep only
// cross-region traffic (source and destination differ) and total it.
service_mesh:mesh_request_count
| where src_region != dst_region
| group using sum
