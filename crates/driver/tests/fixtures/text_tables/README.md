# Text tables written by NumPy

`run.csv`, `column.txt`, `square.txt` and the files the tests expect to fail on (`bad_value.txt`,
`ragged.txt`, `hex.txt` and `underscore.txt`) were written by hand. The files named `numpy_*`
were written by NumPy 2.5.3 on Linux x86-64 from them. The build tests in
[`../../build/text_tables.rs`](../../build/text_tables.rs) read the hand-written files with
`Array.loadtxt` and compare what `Array.savetxt` writes with NumPy's files byte for byte, so the
tests show that Baryon reads and writes text tables as NumPy does (decision 0043) without Python
running in the tests.

To record them again, run this in this directory:

```text
import numpy as np

run = np.loadtxt("run.csv", delimiter=",", skiprows=2, ndmin=2)
np.savetxt("numpy_run.txt", run)
np.savetxt("numpy_run.csv", run, delimiter=",", header="time,energy\npressure", footer="end",
           comments="% ")
np.savetxt("numpy_column.txt", np.loadtxt("column.txt"))
np.savetxt("numpy_first_rows.txt", np.loadtxt("run.csv", delimiter=",", skiprows=2,
                                               max_rows=2, ndmin=2))
```

The messages the tests expect when reading fails are NumPy's, from `numpy.loadtxt` of each
failing file:

```text
bad_value.txt   could not convert string 'x' to float64 at row 1, column 2.
ragged.txt      the number of columns changed from 3 to 2 at row 2; use `usecols` to select a subset and avoid this error
hex.txt         could not convert string '0x10' to float64 at row 0, column 1.
underscore.txt  could not convert string '1_000' to float64 at row 0, column 1.
```

The tests also write `numpy_first_rows.txt` with `newline="\r\n"` and compare it with this file's
lines ended by `"\r\n"`, since the repository keeps every text file's line ends as `"\n"`.
