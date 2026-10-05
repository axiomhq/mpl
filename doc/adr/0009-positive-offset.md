# 9. positive offsets

date: 2026-10-02

## status

proposed

## context

we left forward offsets for later in [source offsets](0008-source-shift.md). this adds them. the sign still matches the direction.

## decision

```mpl
metrics:cpu | offset -1h // read an hour earlier
metrics:cpu | offset +1h // read an hour later
metrics:cpu | offset 1h  // same as +1h
```

keep the same units and limits. `Offset` already stores a signed number, and `Timerange::offset` already moves in either direction.

## consequences

durations too large to fit return an error. offset parameters can wait.
