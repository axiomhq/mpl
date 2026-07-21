param $dataset: Dataset;
param $duration: Duration;
param $__tag: string;
param $array: array;


$dataset:metric
| where __tag == $`__tag`
| align to $duration using avg
