# A worked analysis with DataFrame

This page takes one small pandas analysis and writes it again in Baryon, line for line, so you
can see what carries over and what changes. Then it shows how fast the same operations run on a
table of 10 million rows. It assumes you have read
[Data, files and plots](data-files-and-plots.md), which introduces `DataFrame`.

The data is three days of weather from three stations. Two thermometers failed, so two
temperatures are *missing*: the table has a row for them, but no number.

## The pandas original

```python
import pandas as pd

readings = pd.DataFrame({
    "station": ["Alta", "Alta", "Alta", "Bergen", "Bergen", "Bergen", "Oslo", "Oslo"],
    "day": [1, 2, 3, 1, 2, 3, 1, 2],
    "temperature": [-4.5, None, -2.0, 6.5, 7.0, None, 3.0, 4.5],
    "rain": [0.0, 1.2, 0.4, 12.5, 8.0, 15.5, 2.0, 0.0],
})
print(readings)
print(readings.temperature.isna().sum(), "readings are missing")

readings = readings.assign(fahrenheit=readings.temperature * 1.8 + 32.0)

mild = readings[(readings.temperature > 0.0) & (readings.rain < 10.0)]
print(mild)

summary = readings.groupby("station", as_index=False).agg(
    mean=("temperature", "mean"),
    measured=("temperature", "count"),
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
```

It prints the readings, counts the gaps, adds a Fahrenheit column, chooses the mild dry days,
summarizes each station, joins the stations' elevations and sorts by rain.

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


def main():
    readings: DataFrame[Reading] = DataFrame({
        "station": Array(["Alta", "Alta", "Alta", "Bergen", "Bergen", "Bergen", "Oslo", "Oslo"]),
        "day": Array([1, 2, 3, 1, 2, 3, 1, 2]),
        "temperature": Array([-4.5, None, -2.0, 6.5, 7.0, None, 3.0, 4.5]),
        "rain": Array([0.0, 1.2, 0.4, 12.5, 8.0, 15.5, 2.0, 0.0]),
    })
    print(readings)
    print(readings.temperature.isna().sum(), "readings are missing")

    # Step 1: a new column. A missing temperature stays missing.
    wider = readings.assign(fahrenheit=readings.temperature * 1.8 + 32.0)

    # Step 2: choose rows. A row whose temperature is missing is not chosen.
    mild = wider[(wider.temperature > 0.0) & (wider.rain < 10.0)]
    print(mild)

    # Step 3: one row for each station. The mean skips the missing readings.
    summary = wider.groupby("station").agg(
        mean=("temperature", "mean"),
        measured=("temperature", "count"),
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
  station  day  temperature  rain  fahrenheit
0  Bergen    2          7.0   8.0        44.6
1    Oslo    1          3.0   2.0        37.4
2    Oslo    2          4.5   0.0        40.1
  station  mean  measured  rain
0    Alta -3.25         2   1.6
1  Bergen  6.75         2  36.0
2    Oslo  3.75         2   2.0
  station  mean  measured  rain  elevation
0  Bergen  6.75         2  36.0       12.0
1    Oslo  3.75         2   2.0       <NA>
2    Alta -3.25         2   1.6        3.0
6 complete rows
1.8125
```

Every method call is the one pandas has, with the same arguments. Step by step:

1. **A new column.** `assign` adds `fahrenheit`. Arithmetic on a missing temperature gives a
   missing result, so rows 1 and 5 have no Fahrenheit value either.
2. **Choosing rows.** A comparison with a missing value is itself missing, neither `True` nor
   `False`, and a row whose mask is missing is not chosen. Row 5 (Bergen, day 3, no temperature)
   is left out although nothing says it was cold. `&` follows pandas's rules:
   `False & missing` is `False`, and `True & missing` is missing.
3. **Grouping.** `mean` and `count` skip the missing readings, so each station's mean is over
   the two days it measured.
4. **Joining.** `how="left"` keeps every row of `summary`. Oslo is not in `stations`, so its
   `elevation` is missing, and the compiler gives that column the type `f64 | None` without
   being told.
5. **Taking gaps out.** `dropna()` drops the rows that have one, and `fillna(0.0)` puts a
   number in their place.

## What changed from pandas

- **The columns are declared.** `Reading` and `Station` are *schemas*: structs that name each
  column and its type. `temperature: f64 | None` says a temperature may be missing; `rain: f64`
  says rain never is, and the compiler holds you to it.
- **A column is built from an array.** pandas takes a Python list; Baryon takes
  `Array([...])`, which stores the numbers side by side with no object for each.
- **A missing value is `None`, never `NaN`.** pandas turns the `None` in the list into the
  float `NaN` and prints `NaN`. Baryon keeps it as `None`, in integer and text columns too, and
  prints `<NA>` as pandas does for its own nullable types.
  [Missing values](../reference/missing-values.md) has the rules.
- **A variable keeps one type.** `readings.assign(...)` has one more column than `readings`, so
  it is a different type and needs a new name, `wider`. pandas lets you reuse `readings`.
- **A frame moves.** `assign` takes over the columns of `readings` instead of copying them, so
  the program uses `wider` from then on. Write `readings.copy().assign(...)` to keep both.
- **There is no row index.** pandas prints the mild days as rows 4, 6 and 7, their labels in
  the original table. Baryon numbers the rows of every result from 0. For the same reason
  `groupby` needs no `as_index=False`: the key is always an ordinary column.

Nothing else differs: the two programs print the same numbers.
[DataFrames](../reference/dataframes.md) and
[Sorting, grouping and joining](../reference/dataframe-operations.md) list every operation.

## How fast is it

[`bench/dataframe`](../../bench/dataframe/README.md) runs six operations on a table of 10
million rows in Baryon, pandas and Polars, and checks that all three compute the same answer.
On a 4-core cloud machine, in seconds, fastest of three runs:

| Operation | Baryon | pandas 3.0 | Polars 2.1 |
| --- | --- | --- | --- |
| `filter`: rows where a column is over 0.5 | 0.14 | 0.12 | 0.02 |
| `assign`: a column computed from two others | 0.05 | 0.07 | 0.04 |
| `groupby`: sum and mean for 1000 keys | 3.58 | 0.19 | 0.12 |
| `sort`: `sort_values` by a float column | 3.62 | 1.29 | 0.52 |
| `merge`: inner join to a table of 1000 rows | 4.39 | 0.94 | 0.12 |
| `missing`: a mask and a mean over a column with gaps | 0.22 | 0.14 | 0.02 |

Baryon runs each column operation as one compiled loop on one core, which is why `filter` and
`assign` are level with pandas. Sorting, grouping and joining are first versions written to be
correct and easy to read: each sorts the rows by key with a merge sort on one core, where
pandas and Polars hash the keys and Polars uses every core. These numbers are the starting line
for that work, not the finish. The benchmark's own page says how each number is measured and
how to run it on your machine.
