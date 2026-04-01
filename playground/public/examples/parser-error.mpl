`dev.metrics`:alertmanager_alerts
| map * 5
| align to 5m using max
| group using max
