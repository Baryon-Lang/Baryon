# A worked analysis with DataFrame

This page takes one small pandas analysis and writes it again in Baryon, line for line, so you
can see what carries over and what changes. Then it shows how fast the same operations run on a
table of 10 million rows. It assumes you have read
[Data, files and plots](data-files-and-plots.md), which introduces `DataFrame`.

The data is three days of weather from three stations, in a CSV file. Two thermometers
failed, so two temperatures are *missing*: the file has a row for them, with an empty cell
where the number would be. Save it in the directory you run the programs from.

File `readings.csv`:

```text
station,day,temperature,rain
Alta,1,-4.5,0.0
Alta,2,,1.2
Alta,3,-2.0,0.4
Bergen,1,6.5,12.5
Bergen,2,7.0,8.0
Bergen,3,,15.5
Oslo,1,3.0,2.0
Oslo,2,4.5,0.0
```

## The pandas original

```python
import pandas as pd

readings = pd.read_csv("readings.csv")
print(readings)
print(readings.temperature.isna().sum(), "readings are missing")

readings = readings.assign(fahrenheit=readings.temperature * 1.8 + 32.0, source="manual")

mild = readings[(readings.temperature > 0.0) & (readings.rain < 10.0)]
print(mild)

summary = readings.groupby("station", as_index=False).agg(
    days=("day", "size"),
    measured=("temperature", "count"),
    mean=("temperature", "mean"),
    rain=("rain", "sum"),
)
print(summary)

stations = pd.DataFrame({
    "station": ["Alta", "Bergen", "Tromso"],
    "elevation": [3.0, 12.0, 10.0],
})
joined = summary.merge(stations, on="station", how="left")
print(joined.sort_values("rain", ascending=False))

print(len(readings.dropna()), "complete rows")
print(readings.temperature.fillna(0.0).mean())

joined.to_csv("summary.csv", index=False)
```

It reads and prints the readings, counts the gaps, adds two columns, chooses the mild dry
days, summarizes each station, joins the stations' elevations, sorts by rain and writes the
result to a file.

## The same analysis in Baryon

File `weather/main.byn`:

```python
class Reading(Struct):
    station: str
    day: i64
    temperature: f64 | None
    rain: f64


class Station(Struct):
    station: str
    elevation: f64


def main() -> Raises[None, OSError | ValueError]:
    readings: DataFrame[Reading] = DataFrame.read_csv("readings.csv")
    print(readings)
    print(readings.temperature.isna().sum(), "readings are missing")

    # Step 1: new columns. A missing temperature stays missing; one value fills every row.
    wider = readings.assign(fahrenheit=readings.temperature * 1.8 + 32.0, source="manual")

    # Step 2: choose rows. A row whose temperature is missing is not chosen.
    mild = wider[(wider.temperature > 0.0) & (wider.rain < 10.0)]
    print(mild)

    # Step 3: one row for each station. The mean skips the missing readings.
    summary = wider.groupby("station").agg(
        days=("day", "size"),
        measured=("temperature", "count"),
        mean=("temperature", "mean"),
        rain=("rain", "sum"),
    )
    print(summary)

    # Step 4: join a second table, keeping stations it does not list.
    stations: DataFrame[Station] = DataFrame({
        "station": Array(["Alta", "Bergen", "Tromso"]),
        "elevation": Array([3.0, 12.0, 10.0]),
    })
    joined = summary.merge(stations, on="station", how="left")
    print(joined.sort_values("rain", ascending=False))

    # Step 5: take the gaps out, or fill them.
    print(len(wider.dropna()), "complete rows")
    print(wider.temperature.fillna(0.0).mean())

    # Step 6: write the result.
    joined.to_csv("summary.csv")
```

```text
$ baryon run weather
  station  day  temperature  rain
0    Alta    1         -4.5   0.0
1    Alta    2         <NA>   1.2
2    Alta    3         -2.0   0.4
3  Bergen    1          6.5  12.5
4  Bergen    2          7.0   8.0
5  Bergen    3         <NA>  15.5
6    Oslo    1          3.0   2.0
7    Oslo    2          4.5   0.0
2 readings are missing
  station  day  temperature  rain  fahrenheit  source
0  Bergen    2          7.0   8.0        44.6  manual
1    Oslo    1          3.0   2.0        37.4  manual
2    Oslo    2          4.5   0.0        40.1  manual
  station  days  measured  mean  rain
0    Alta     3         2 -3.25   1.6
1  Bergen     3         2  6.75  36.0
2    Oslo     2         2  3.75   2.0
  station  days  measured  mean  rain  elevation
0  Bergen     3         2  6.75  36.0       12.0
1    Oslo     2         2  3.75   2.0       <NA>
2    Alta     3         2 -3.25   1.6        3.0
6 complete rows
1.8125
```

It also writes `summary.csv`, byte for byte the file pandas writes:

```text
station,days,measured,mean,rain,elevation
Alta,3,2,-3.25,1.6,3.0
Bergen,3,2,6.75,36.0,12.0
Oslo,2,2,3.75,2.0,
```

Every method call is the one pandas has, with the same arguments. Step by step:

1. **New columns.** `assign` adds `fahrenheit` and `source`. Arithmetic on a missing
   temperature gives a missing result, so rows 1 and 5 have no Fahrenheit value either. One
   value, `"manual"`, fills every row, as in pandas.
2. **Choosing rows.** A comparison with a missing value is itself missing, neither `True` nor
   `False`, and a row whose mask is missing is not chosen. Row 5 (Bergen, day 3, no temperature)
   is left out although nothing says it was cold. `&` follows pandas's rules:
   `False & missing` is `False`, and `True & missing` is missing.
3. **Grouping.** `size` counts a station's rows and `count` the temperatures it measured;
   `mean` skips the missing readings, so each station's mean is over the days it measured.
4. **Joining.** `how="left"` keeps every row of `summary`. Oslo is not in `stations`, so its
   `elevation` is missing, and the compiler gives that column the type `f64 | None` without
   being told.
5. **Taking gaps out.** `dropna()` drops the rows that have one, and `fillna(0.0)` puts a
   number in their place.
6. **Writing.** `to_csv` writes the header and the rows, with an empty cell for Oslo's missing
   elevation.

## What changed from pandas

- **The columns are declared.** `Reading` and `Station` are *schemas*: structs that name each
  column and its type. `temperature: f64 | None` says a temperature may be missing; `rain: f64`
  says rain never is, and the compiler holds you to it. `read_csv` needs the schema, because a
  compiled program cannot learn a file's columns when it runs: the annotation
  `readings: DataFrame[Reading]` tells it which columns to read and how. An empty cell under
  `rain` would stop the program with a `ValueError` that names the column and the row.
- **Reading a file can fail, and the function says so.** `main` lists `OSError` and
  `ValueError` in its `Raises`, the two ways `read_csv` fails. [Errors](../reference/errors.md)
  explains `Raises`.
- **A column is built from an array.** For the `stations` table pandas takes a Python list;
  Baryon takes `Array([...])`, which stores the numbers side by side with no object for each.
- **A missing value is `None`, never `NaN`.** pandas reads the empty cell as the float `NaN`
  and prints `NaN`. Baryon reads it as `None`, in integer and text columns too, and prints
  `<NA>` as pandas does for its own nullable types.
  [Missing values](../reference/missing-values.md) has the rules.
- **A variable keeps one type.** `readings.assign(...)` has two more columns than `readings`,
  so it is a different type and needs a new name, `wider`. pandas lets you reuse `readings`.
- **A frame moves.** `assign` takes over the columns of `readings` instead of copying them, so
  the program uses `wider` from then on. Write `readings.copy().assign(...)` to keep both.
- **There is no row index.** pandas prints the mild days as rows 4, 6 and 7, their labels in
  the original table. Baryon numbers the rows of every result from 0. For the same reason
  `groupby` needs no `as_index=False`, and `to_csv` no `index=False`: the key is always an
  ordinary column, and there is no index to write.

Nothing else differs: the two programs print the same numbers and write the same file.

## More that carries over

The analysis above uses a small part of what a frame can do. These are written as in pandas
too, and each has its page in the reference:

| pandas | Where it is described |
| --- | --- |
| `df.drop("a", axis=1)`, `df.rename(columns={...})`, `df[["a", "b"]]` | [Choosing and changing columns](../reference/dataframes.md#choosing-and-changing-columns) |
| `sort_values(["a", "b"], ascending=[True, False], na_position="first")` | [Sorting, grouping and joining](../reference/dataframe-operations.md) |
| `agg` with `sum`, `mean`, `min`, `max`, `count`, `size`, `nunique`, `first`, `last`, `std`, `var`, `median` | [Grouping](../reference/dataframe-operations.md#grouping) |
| `merge` with `how=`, `left_on=`, `right_on=` and `suffixes=` | [Sorting, grouping and joining](../reference/dataframe-operations.md) |
| `drop_duplicates(keep="last")`, `value_counts(normalize=True)`, `describe(percentiles=[0.1, 0.9])` | [Sorting, grouping and joining](../reference/dataframe-operations.md) |
| `isna`, `notna`, `fillna`, `dropna`, arithmetic and masks with missing values | [Missing values in frames](../reference/missing-values-in-frames.md) |
| `DataFrame.read_csv(path)`, `df.to_csv(path)` | [CSV files](../reference/csv-files.md) |

[What builds today](../reference/status.md) lists what is not built yet, such as the keyword
arguments of `read_csv`.

## How fast is it

[`bench/dataframe`](../../bench/dataframe/README.md) runs nine operations on a table of 10
million rows in Baryon, pandas and Polars, and checks that they compute the same answer. On a
4-core cloud machine, in seconds, fastest of three runs:

| Operation | Baryon | pandas 3.0 | Polars 2.1 |
| --- | --- | --- | --- |
| `filter`: rows where a column is over 0.5 | 0.11 | 0.12 | 0.02 |
| `assign`: a column computed from two others | 0.03 | 0.07 | 0.03 |
| `groupby`: sum and mean for 1000 keys | 0.87 | 0.18 | 0.10 |
| `sort`: `sort_values` by a float column | 0.91 | 1.26 | 0.47 |
| `merge`: inner join to a table of 1000 rows | 0.95 | 0.92 | 0.09 |
| `missing`: a mask and a mean over a column with gaps | 0.20 | 0.15 | 0.03 |
| `counts`: `size` and `nunique` for 1000 keys | 1.65 | 1.82 | not measured |
| `write`: `to_csv` of three columns, 424 MB | 25.9 | 30.9 | not measured |
| `read`: `read_csv` of that file | 4.0 | 2.7 | not measured |

The Baryon and pandas columns are from 2026-11-19. The Polars column is from the day before,
on the same kind of machine; Polars was not installed where the last three operations were
added.

Baryon runs each operation on one core, as pandas does. It is ahead of pandas in five of the
nine and close behind in `merge`. It is well behind in three: `groupby`, where about half the
time goes to sums that are correctly rounded
([Grouping](../reference/dataframe-operations.md#grouping)), which pandas's are not;
`missing`; and `read`, which copies each float's text before reading it. Polars is ahead
where it was measured because it uses every core. Running Baryon's column loops on every core
is the next step, and these numbers are its starting line. The benchmark's own page says how
each number is measured and how to run it on your machine.

## Where to go next

[Special functions](special-functions.md) is the last page of this guide: `scipy.special`'s
functions in Baryon, moving a program from SciPy, and their speed.
