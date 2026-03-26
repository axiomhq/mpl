// sum(kube_pod_status_ready) by (pod)
// *
// on(pod)
// group_left(created_by_kind)
// kube_pod_info{}

set strict;

test:kube_pod_status_ready
| group by pod using sum
| join created_by_kind from test:kube_pod_info by pod
