# Array files written by NumPy

These files were written by NumPy 2.5.3 on Linux x86-64. The build tests in
[`../../build/array_files.rs`](../../build/array_files.rs) read them with `Array.fromfile` and
compare what `a.tofile` writes with them byte for byte, so the tests show that Baryon and NumPy
share array files (decision 0036) without Python running in the tests.

To record them again, run this in this directory:

```text
import numpy as np

np.array([1.5, -2.25, 1e300, 3.0]).tofile("f64.bin")
np.fromfile("f64.bin")[::2].tofile("every_other.bin")
np.array([1, -2, 300], dtype=np.int16).tofile("i16.bin")
np.array([True, False, True]).tofile("bool.bin")
np.array([1 + 2j, -0.5j]).tofile("complex128.bin")
particle = np.dtype([("mass", "f8"), ("charge", "i4"), ("alive", "?")], align=True)
np.array([(1.5, -1, True), (2.5, 2, False)], dtype=particle).tofile("particles.bin")
grid = np.array([[0.0, 1.0, 2.0], [10.0, 11.0, 12.0]])
grid.tofile("grid.bin")
grid[:, 1].tofile("column.bin")
```

`particles.bin` uses `align=True`, which lays the record out as C does and as Baryon lays out
`class Particle(Struct, Copy)` with the same fields: 16 bytes, with 3 bytes of padding at the end.
