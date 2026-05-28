dataset:metric
| where host == "badger"
| extend cake = true
