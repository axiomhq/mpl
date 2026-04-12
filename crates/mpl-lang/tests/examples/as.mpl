// avg_over_time(
//    (
//       max(
//           ingest_pressure{
//              time_window = "1m",
//               service =~ "axiomdb-[a-f]"
//           }
//        ) <bool 0.4
//    )[7d:]
// )

`test-with-minus.com`:ingest_pressure as snot
| where time_window == "1m"
| where service == #/axiomdb-[a-f]/
| group using max
| as cookie
| map is::lt(0.4)
| align to 7d using avg
| as cake
