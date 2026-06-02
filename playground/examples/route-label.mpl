// Tag references work anywhere an expression is allowed, including string
// interpolation and `extend`. Here each series is stamped with a "src->dst"
// route label built from its own tag values.
service_mesh:mesh_request_count
| extend route = "${ src_region }->${ dst_region }"
