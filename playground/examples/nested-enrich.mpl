// sum(kube_pod_status_ready) by (pod)
// *
// on(pod)
// group_left(node)
// kube_pod_info
// *
// on(node)
// group_left(internal_ip)
// kube_node_info


test:kube_pod_status_ready
| group by pod using sum
| join node from test:kube_pod_info by pod
| join internal_ip from test:kube_node_info by node