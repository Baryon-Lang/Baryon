"""The same operations as main.byn in pandas and in Polars, each timed alone.

Usage: python3 frames.py pandas|polars ROWS RUNS CSV. Prints one line for each operation: its
name, the fastest of RUNS times in seconds, and the same checksum main.byn prints. CSV is the
path of the file `write` makes and `read` reads.

The Polars list has the first six operations only: `counts`, `write` and `read` were added on
a machine without Polars, and an operation is not added here without running it.
"""

import sys
import time

import numpy as np


def columns(rows):
    """The three columns main.byn builds, with the same numbers."""
    i = np.arange(rows, dtype=np.int64)
    key = (i * 7919) % 1000
    value = ((i * 2654435761) % 1000003) / 1000003.0
    other = ((i * 40503) % 999983) / 999983.0
    return key, value, other


def sector_columns():
    """The two columns of the table to join to."""
    i = np.arange(1000, dtype=np.int64)
    return i, (i % 7) + 0.5


def pandas_operations(rows, path):
    import pandas as pd

    key, value, other = columns(rows)
    table = pd.DataFrame({"key": key, "value": value, "other": other})
    sector_key, weight = sector_columns()
    sectors = pd.DataFrame({"key": sector_key, "weight": weight})

    def filter_rows():
        kept = table[table.value > 0.5]
        return f"{len(kept)} {kept.other.sum():.6f}"

    def assign():
        wider = table.assign(total=table.value * 2.0 + table.other)
        return f"{wider.total.sum():.6f}"

    def groupby():
        groups = table.groupby("key").agg(total=("value", "sum"), mean=("other", "mean"))
        return f"{len(groups)} {groups.total.sum():.6f} {groups['mean'].sum():.6f}"

    def sort():
        ordered = table.sort_values("value", kind="stable")
        return f"{ordered.other.iloc[0]:.6f} {ordered.other.iloc[rows - 1]:.6f}"

    def merge():
        joined = table.merge(sectors, on="key")
        return f"{len(joined)} {(joined.value * joined.weight).sum():.6f}"

    gaps = pd.DataFrame({"bid": pd.array(value, dtype="Float64"), "other": other})
    gaps.loc[::10, "bid"] = pd.NA

    def missing():
        kept = gaps[gaps.bid > 0.5]
        return f"{len(kept)} {kept.other.sum():.6f} {gaps.bid.mean():.6f}"

    def counts():
        groups = table.groupby("key").agg(rows=("value", "size"), different=("other", "nunique"))
        return f"{len(groups)} {groups.rows.sum()} {groups.different.sum()}"

    def write():
        table.to_csv(path, index=False)
        return f"{len(table)}"

    def read():
        loaded = pd.read_csv(path)
        return f"{len(loaded)} {loaded.value.sum():.6f} {loaded.key.sum()}"

    return [("filter", filter_rows), ("assign", assign), ("groupby", groupby),
            ("sort", sort), ("merge", merge), ("missing", missing),
            ("counts", counts), ("write", write), ("read", read)]


def polars_operations(rows):
    import polars as pl

    key, value, other = columns(rows)
    table = pl.DataFrame({"key": key, "value": value, "other": other})
    sector_key, weight = sector_columns()
    sectors = pl.DataFrame({"key": sector_key, "weight": weight})

    def filter_rows():
        kept = table.filter(pl.col("value") > 0.5)
        return f"{len(kept)} {kept['other'].sum():.6f}"

    def assign():
        wider = table.with_columns(total=pl.col("value") * 2.0 + pl.col("other"))
        return f"{wider['total'].sum():.6f}"

    def groupby():
        groups = table.group_by("key").agg(
            total=pl.col("value").sum(), mean=pl.col("other").mean()
        )
        return f"{len(groups)} {groups['total'].sum():.6f} {groups['mean'].sum():.6f}"

    def sort():
        ordered = table.sort("value", maintain_order=True)
        return f"{ordered['other'][0]:.6f} {ordered['other'][rows - 1]:.6f}"

    def merge():
        joined = table.join(sectors, on="key")
        return f"{len(joined)} {(joined['value'] * joined['weight']).sum():.6f}"

    gaps = pl.DataFrame({"bid": value, "other": other}).with_columns(
        bid=pl.when(pl.int_range(pl.len()) % 10 == 0).then(None).otherwise(pl.col("bid"))
    )

    def missing():
        kept = gaps.filter(pl.col("bid") > 0.5)
        return f"{len(kept)} {kept['other'].sum():.6f} {gaps['bid'].mean():.6f}"

    return [("filter", filter_rows), ("assign", assign), ("groupby", groupby),
            ("sort", sort), ("merge", merge), ("missing", missing)]


def main():
    library, rows, runs = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    if library == "pandas":
        operations = pandas_operations(rows, sys.argv[4])
    else:
        operations = polars_operations(rows)
    for name, operation in operations:
        best = float("inf")
        for _ in range(runs):
            start = time.perf_counter()
            checksum = operation()
            best = min(best, time.perf_counter() - start)
        print(name, f"{best:.4f}", checksum)


main()
