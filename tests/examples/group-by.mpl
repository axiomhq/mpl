`dataset`:`handling_seconds`
| where `service.name` == "fancy-schmancy-service"
| align to 5m using avg
| group by method using avg
