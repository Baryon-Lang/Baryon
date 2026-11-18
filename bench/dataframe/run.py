"""Times the DataFrame benchmark in Baryon, pandas and Polars and prints one table.

Usage, from the repository root:

    python3 bench/dataframe/run.py [--rows N] [--runs N] [--baryon PATH]

Baryon has no clock yet, so each Baryon operation is timed as a whole process, and the time of
a process that only builds the table (`setup`, or `missing_setup` for the table with gaps) is
taken off. pandas and Polars time each operation alone, inside one process, in frames.py. A
library that is not installed is left out of the table.
"""

import argparse
import pathlib
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
OPERATIONS = ["filter", "assign", "groupby", "sort", "merge", "missing"]


def timed(command):
    """Runs `command` and returns its wall time in seconds and what it printed."""
    start = time.perf_counter()
    done = subprocess.run(command, check=True, capture_output=True, text=True)
    return time.perf_counter() - start, done.stdout.strip()


def baryon_times(baryon, rows, runs):
    """Each operation's fastest time less the fastest table-building time, and its checksum."""
    program = HERE.parents[1] / "target" / "bench" / "dataframe"
    program.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run([baryon, "build", "--release", HERE / "baryon", program], check=True)

    def best(operation):
        runs_of = [timed([program, operation, str(rows)]) for _ in range(runs)]
        return min(seconds for seconds, _ in runs_of), runs_of[0][1]

    setup, _ = best("setup")
    missing_setup, _ = best("missing_setup")
    results = {}
    for operation in OPERATIONS:
        seconds, checksum = best(operation)
        base = missing_setup if operation == "missing" else setup
        results[operation] = (max(seconds - base, 0.0), checksum)
    return results


def library_times(library, rows, runs):
    """What frames.py measured for `library`, or None when the library is not installed."""
    command = [sys.executable, HERE / "frames.py", library, str(rows), str(runs)]
    done = subprocess.run(command, capture_output=True, text=True)
    if done.returncode != 0:
        return None
    results = {}
    for line in done.stdout.splitlines():
        name, seconds, checksum = line.split(" ", 2)
        results[name] = (float(seconds), checksum)
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--rows", type=int, default=10_000_000)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--baryon", default="baryon")
    options = parser.parse_args()

    baryon = baryon_times(options.baryon, options.rows, options.runs)
    others = {
        name: library_times(name, options.rows, options.runs) for name in ["pandas", "polars"]
    }
    print(f"{'operation':<10}{'Baryon':>10}{'pandas':>10}{'Polars':>10}  checksum")
    agree = True
    for operation in OPERATIONS:
        seconds, checksum = baryon[operation]
        cells = [f"{seconds:>10.3f}"]
        verdict = "identical"
        for results in others.values():
            if results is None:
                cells.append(f"{'absent':>10}")
                continue
            cells.append(f"{results[operation][0]:>10.3f}")
            if results[operation][1] != checksum:
                verdict = "DISAGREE"
                agree = False
        print(f"{operation:<10}{''.join(cells)}  {verdict}")
    sys.exit(0 if agree else 1)


main()
