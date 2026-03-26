param $dataset: dataset;
param $duration: duration;
param $__tag: string;

$dataset:metric
| where __tag == $__tag
| align to $duration using avg
