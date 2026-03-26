// max(
//     label_replace(
//         (
//             sum(
//               node_namespace_pod_container:container_cpu_usage_seconds_total:sum_irate{
//                   namespace=~"(axiom|cloud).*",
//                   pod=~"axiom-ingest-.*"
//               }
//             ) by (pod)
//             /
//             sum(
//               cluster:namespace:pod_cpu:active:kube_pod_container_resource_requests{
//                       namespace=~"(axiom|cloud).*",
//                       pod=~"axiom-ingest-.*",
//                       resource="cpu"}
//              ) by (pod)
//         ),
//         "service", "$1", "pod", "(.+)-.+-.+"
//     )
// ) by (service)

// note node_namespace_pod_container:container_cpu_usage_seconds_total:sum_irate are pre-computed
// resources by the prometheus ingest pipeline they do not get send to us at the moment

(
    test:container_cpu_usage_seconds_total
    | where namespace == #/(axiom|cloud).*/
    | where pod == #/axiom-ingest-.*/
    | group by pod using sum,
    test:kube_pod_container_resource_requests
    | where namespace == #/(axiom|cloud).*/
    | where pod == #/axiom-ingest-.*/
    | where resource == "cpu"
    | group by pod using sum,
)
| compute cpu_usage using /
| replace service = pod ~ #s/(.+)-.+-.+/$1/
| group by service using max
