param $dataset: Dataset;
param $duration: Duration;
param $tag: Option<string>;

$dataset:metric
| ifdef($tag) { where __tag == $tag }
| align to $duration using avg
