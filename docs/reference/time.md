# Clocks: `time`

[What builds today](status.md#files-and-processes) says which parts of this page the compiler
builds into programs yet.

A simulation that reports how long each phase took needs a clock. Baryon's `time` module keeps
Python's name and three of its functions, with Python's meanings (roadmap 1.13.19). Each takes no
arguments and gives an `f64` of seconds.

| Function | Gives | Use it for |
| --- | --- | --- |
| `time.perf_counter()` | Seconds on a clock that never goes back, with the finest resolution the system has | Timing a piece of a program |
| `time.monotonic()` | The same clock as `perf_counter` | Timeouts and intervals, as in Python |
| `time.time()` | Seconds since 1970-01-01 00:00:00 UTC, as the system's date has it | Stamping a result with when it was made |

```python
import time


def main():
    start = time.perf_counter()
    total = 0.0
    for i in range(1, 10000001):
        total += 1.0 / (f64(i) * f64(i))
    elapsed = time.perf_counter() - start
    print(f"sum {total:.6f} in {elapsed:.3f} s")
```

This adds ten million terms of the series for pi squared over 6 and prints a line such as
`sum 1.644934 in 0.010 s`. The time depends on the machine and the build: that line is from a
`--release` build on a Linux x86_64 server, where a debug build took 0.09 s.

## Which clock to use

`perf_counter` and `monotonic` never go back, and setting the system's date does not move them.
Only the difference of two readings means anything: the zero is the program's first reading of
the clock, where Python's is undefined (on Linux, when the machine started). Neither counts time
the machine sleeps. The clock is `CLOCK_MONOTONIC` on Linux, the one Python reads for both
functions there, with a resolution of a nanosecond, and `CLOCK_UPTIME_RAW` on macOS.

`time()` reads the system's date, which moves when the date is set, by hand or by a time server, so
the difference of two readings can be wrong or even negative. Time a computation with
`perf_counter`, and keep `time()` for saying when it ran. An `f64` holds today's date to about a
quarter of a microsecond, as in Python.

The functions can be imported by name too: `from time import perf_counter`.

## What is not built yet

The rest of Python's `time` module is not built: `sleep`, the `_ns` forms such as
`perf_counter_ns`, `process_time`, and the calendar functions such as `localtime` and `strftime`.
Using one is a compile error that names it. A clock read without its call, such as
`f = time.perf_counter`, is an error too: only calls of the clocks are built.
