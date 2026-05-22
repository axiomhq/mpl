param $dataset: Dataset;
param $duration: Duration;
param $tag: Option<string>;

$dataset:metric
| ifdef($tag) { where __tag == $tag } else { where __tag == "default" }
| align to $duration using avg
