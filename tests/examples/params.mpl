param $dataset: Dataset;
param $duration: Duration;
param $timestamp: Timestamp;
param $__tag: string;

$dataset:metric
| where __tag == $__tag
| align to $duration using avg
| extend timestamp = "${ $timestamp }"
