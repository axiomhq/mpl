// Tag-vs-tag filter: a tag can be referenced as a value, so you can compare
// two tags on the same series. This keeps only intra-region traffic, where a
// request's source and destination region are the same.
service_mesh:mesh_request_count
| where src_region == dst_region
