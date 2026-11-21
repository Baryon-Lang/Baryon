# DataFrame benchmark

Nine `DataFrame` operations on a table of 10 million rows, written in Baryon, in pandas and,
for the first six, in Polars. It is the measurement roadmap item 3.13.10 asks for; item
3.13.20 added the last three operations. The parallel and
fused versions of the column loops start from these numbers, as the faster sorts of item
3.13.12 did: performance work begins with a benchmark that shows the problem and ends with one
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

The run fails if a library's checksum differs from Baryon's, or if the CSV file Baryon writes
is not byte for byte the file pandas writes. `write` and `read` use a temporary directory
that needs 0.5 GB for each library at 10 million rows.

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
| `counts` | `groupby("key")` with the `size` of each group and the `nunique` of `other` | groups, sum of each result column |
| `write` | `table.to_csv(path)`, 424 MB at 10 million rows | rows, and the file compared with pandas's |
| `read` | `DataFrame.read_csv(path)` of that file | rows, sum of `value`, sum of `key` |

## Results

Linux x86-64, a shared cloud machine with 4 cores (Intel Xeon, 2.1 GHz) and 15 GB of memory,
2026-11-21: pandas 3.0.5 on NumPy 2.5.3, `baryon build --release`. Seconds, fastest of 3. Your
numbers will differ. Polars was not installed on this machine, so its column is `absent`; its
numbers for the first six operations are in the run of 2026-11-18, below.

```text
operation     Baryon    pandas    Polars  checksum
filter         0.094     0.121    absent  identical
assign         0.049     0.069    absent  identical
groupby        0.242     0.188    absent  identical
sort           0.911     1.137    absent  identical
merge          0.926     0.782    absent  identical
missing        0.215     0.150    absent  identical
counts         1.544     1.622    absent  identical
write         24.915    29.302    absent  identical
read           3.938     2.515    absent  identical
```

What they say:

- `filter`, `assign` and `missing` are single compiled loops over the columns. The first two
  are ahead of pandas on one core; `missing` is behind it.
- `sort` is ahead of pandas and `merge` a little behind it. `sort_values` by a column of `f64`
  deals the values into buckets and sorts each one, and half of what is left is copying the
  three columns into their new order. `merge` sorts the keys of both frames by counting.
- `groupby` is 1.3 times behind pandas, and its sums are still correctly rounded, where
  pandas's are compensated but not exact. It was five times behind, 0.870 s against 0.177 s,
  on 2026-11-19, before roadmap 3.13.21. Three things changed, none of them a result: a sum
  is kept exactly as two floats while that is possible, which it is for numbers of like
  size; a sum or mean by one key of whole numbers reads the rows once as they are stored,
  where it reached each group's rows through the sorted positions and missed the cache for
  nearly every one; and the start of each group is found without reading every row. What
  is left is mostly the sort of the rows by key, which the result's key column still needs.
- `counts` is ahead of pandas. `nunique` numbers the column's values with a sort; `size`
  needs only the groups. How the time divides between them has not been measured.
- `write` is ahead of pandas and writes the same bytes, 20 million floats among them, each
  in its shortest text that reads back exactly.
- `read` is 1.6 times behind pandas. It was 2.6 times behind, 7.659 s against 2.909 s, on
  2026-11-19 before roadmap 3.14.4.3, which stopped copying each cell of a whole
  number and stopped `float` and `int` allocating for text without underscores. What is left
  is one copy of each cell of a float, made so that `float` can read it as text, and the index
  of the cells the reader builds before the columns.

### With Polars, 2026-11-18

The same machine type, with Polars 2.1.0, before `counts`, `write` and `read`
were added:

```text
operation     Baryon    pandas    Polars  checksum
filter         0.083     0.121     0.019  identical
assign         0.082     0.069     0.033  identical
groupby        1.106     0.238     0.101  identical
sort           1.142     1.355     0.471  identical
merge          1.056     0.979     0.086  identical
missing        0.247     0.139     0.026  identical
```

Polars is 2 to 12 times ahead of Baryon, with every core and hash tables. The Baryon and
pandas columns differ between the two days by what a shared machine gives from one run to
the next.

### Before roadmap item 3.13.12

The first measurement, on 2026-11-18, which the faster sorts started from:

```text
operation     Baryon    pandas    Polars  checksum
filter         0.137     0.121     0.019  identical
assign         0.052     0.065     0.035  identical
groupby        3.584     0.194     0.120  identical
sort           3.615     1.294     0.523  identical
merge          4.392     0.936     0.116  identical
missing        0.224     0.141     0.024  identical
```

Then all three sorted row positions with one merge sort that read the column through the
position array, which misses the processor's cache at every comparison once a column is larger
than it. Item 3.13.12 replaced it (`crates/driver/library/frame_order.byn`).

## How the time is measured

Baryon has no clock module yet, so the Baryon program runs one operation for each process, and
`run.py` times the whole process. A process that only builds the table (`setup`, or
`missing_setup` for the table with gaps) is timed the same way, and its fastest time is taken
off. `read` builds no table, so its time is the whole process. At 10 million rows building the
table takes about 0.2 seconds, so an operation under about 0.05 seconds is measured to no
better than a few hundredths of a second. `frames.py` times each pandas and Polars operation
alone with `time.perf_counter`, after the table is built.

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
| `frames.py` | The same operations in pandas, and the first six in Polars, timed |
| `run.py` | Builds and times the Baryon program, runs `frames.py`, prints the table |

`crates/driver/tests/build/frame_benchmark.rs` builds the Baryon program in CI and checks its
checksums against pandas's at 1000 rows, with the first lines of the file `write` makes.
