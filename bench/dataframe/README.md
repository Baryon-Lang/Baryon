# DataFrame benchmark

Six `DataFrame` operations on a table of 10 million rows, written three times: in Baryon, in
pandas and in Polars. It is the measurement roadmap item 3.13.10 asks for. The parallel and
fused versions of the column loops, and faster sorting, grouping and joining, start from these
numbers: performance work begins with a benchmark that shows the problem and ends with one
that shows the gain.

## Run it

You need a built `baryon`, Python 3 with NumPy, and pandas and Polars if you want their
columns. A library that is not installed shows as `absent`.

```sh
cargo build --release --package driver
python3 bench/dataframe/run.py --baryon target/release/baryon
```

| Option | Effect | Default |
| --- | --- | --- |
| `--rows <n>` | Rows in the table | 10000000 |
| `--runs <n>` | Runs of each operation; the fastest counts | 3 |
| `--baryon <path>` | The compiler to build the Baryon program with | `baryon` on the `PATH` |

The run fails if a library's checksum differs from Baryon's.

## The table and the operations

Every library builds the same three columns from the row number `i`, with no random numbers,
so the checksums can be compared digit for digit:

| Column | Value |
| --- | --- |
| `key` | `(i * 7919) % 1000`, so 1000 different keys |
| `value` | `((i * 2654435761) % 1000003) / 1000003.0`, between 0 and 1 |
| `other` | `((i * 40503) % 999983) / 999983.0`, between 0 and 1 |

| Operation | What it does | Checksum |
| --- | --- | --- |
| `filter` | `table[table.value > 0.5]` | rows kept, sum of their `other` |
| `assign` | `table.assign(total=table.value * 2.0 + table.other)` | sum of `total` |
| `groupby` | `groupby("key")` with the sum of `value` and the mean of `other` | groups, sum of each result column |
| `sort` | `sort_values("value")`, stable | `other` in the first and last row |
| `merge` | inner `merge` on `key` with a table of 1000 rows, one for each key | rows, sum of `value * weight` |
| `missing` | `value` with every tenth row missing as `bid`: `gaps[gaps.bid > 0.5]` and `gaps.bid.mean()` | rows kept, sum of their `other`, the mean |

## Results

Linux x86-64, a shared cloud machine with 4 cores (Intel Xeon, 2.1 GHz) and 15 GB of memory,
2026-11-18: pandas 3.0.5 on NumPy 2.5.3, Polars 2.1.0, `baryon build --release`. Seconds,
fastest of 3. Your numbers will differ.

```text
operation     Baryon    pandas    Polars  checksum
filter         0.137     0.121     0.019  identical
assign         0.052     0.065     0.035  identical
groupby        3.584     0.194     0.120  identical
sort           3.615     1.294     0.523  identical
merge          4.392     0.936     0.116  identical
missing        0.224     0.141     0.024  identical
```

What they say:

- `filter`, `assign` and `missing` are single compiled loops over the columns, and are level
  with pandas on one core. Polars is ahead because it uses every core.
- `groupby`, `sort` and `merge` are far behind. All three sort row positions with a merge sort
  written in Baryon (`crates/driver/library/frames.byn`), on one core, comparing through the
  position array. pandas and Polars group and join with hash tables.

## How the time is measured

Baryon has no clock module yet, so the Baryon program runs one operation for each process, and
`run.py` times the whole process. A process that only builds the table (`setup`, or
`missing_setup` for the table with gaps) is timed the same way, and its fastest time is taken
off. At 10 million rows building the table takes about 0.2 seconds, so an operation under
about 0.05 seconds is measured to no better than a few hundredths of a second. `frames.py`
times each pandas and Polars operation alone with `time.perf_counter`, after the table is
built.

Known asymmetries:

- Baryon and pandas run on one core. Polars uses all of them.
- The pandas sort asks for `kind="stable"` and the Polars sort for `maintain_order=True`,
  because Baryon's `sort_values` is always stable.
- The pandas table with gaps uses the nullable `Float64` type, pandas's closest match to
  Baryon's `f64 | None`.

## Layout

| Path | Contents |
| --- | --- |
| `baryon/main.byn` | The Baryon program: `main.byn <operation> <rows>` prints the checksum |
| `frames.py` | The same operations in pandas and Polars, timed |
| `run.py` | Builds and times the Baryon program, runs `frames.py`, prints the table |

`crates/driver/tests/build/frame_benchmark.rs` builds the Baryon program in CI and checks its
checksums against pandas's at 1000 rows.
