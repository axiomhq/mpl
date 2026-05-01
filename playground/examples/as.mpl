// avg_over_time(
//    (
//       max(
//           ingest_pressure{
//              time_window = "1m",
//               service =~ "axiomdb-[a-f]"
//           }
//        ) <bool 100
//    )[5m:]
// )

`com.app.test`:ingest_pressure
| where time_window == "1m"
| where service == #/axiomdb-[a-f]/
| group using max
| map is::lt(100)
| align to 5m using avg
| as cake
