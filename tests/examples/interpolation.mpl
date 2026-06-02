param $host: string;
dataset:metric
| extend url = "http://${ $host }:${ 8080 }?id=${ id }"
