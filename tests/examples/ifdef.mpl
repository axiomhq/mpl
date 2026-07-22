param $dataset: Dataset;
param $duration: Duration;
param $tag: Option<string>;
param $array: Option<array>;

$dataset:metric
| ifdef($tag) { where __tag == $tag }
| ifdef($array) { where __array in $array }
| align to $duration using avg
