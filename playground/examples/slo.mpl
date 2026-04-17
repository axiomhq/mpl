// avg_over_time(
//    (
//       max(
//           ingest_pressure{
//              time_window = "1m",
//               service =~ "axiomdb-[a-f]"
//           }
//        ) <bool 100
//    )[10m:]
// )

`com.app.test`:ingest_pressure
| where time_window == "1m"
| where service == #/axiomdb-[a-f]/
| group using max
| map is::lt(100)
| align to 10m using avg
