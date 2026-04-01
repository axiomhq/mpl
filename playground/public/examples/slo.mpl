// avg_over_time(
//    (
//       max(
//           ingest_pressure{
//              time_window = "1m",
//               service =~ "node-[a-f]"
//           }
//        ) <bool 0.4
//    )[7d:]
// )

`test-with-minus.com`:ingest_pressure
| where time_window == "1m"
| where service == #/node-[a-f]/
| group using max
| map is::lt(0.4)
| align to 7d using avg
