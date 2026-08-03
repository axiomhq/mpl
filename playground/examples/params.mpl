param $dataset: Dataset;
param $duration: Duration;
param $code: int;

$dataset:http_requests_total
| where code == $code
| align to $duration using avg
