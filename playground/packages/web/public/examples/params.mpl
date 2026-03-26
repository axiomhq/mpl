param $dataset: dataset;
param $metric: metric;
param $duration: duration;
param $tag: string;

$dataset:$metric
| where tag == $tag
| align to $duration using avg
