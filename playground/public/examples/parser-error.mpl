`dev.metrics`:alertmanager_alerts
| map * 5
| align to 15s using max
| group using max
