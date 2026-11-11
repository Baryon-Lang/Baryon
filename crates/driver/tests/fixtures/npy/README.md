# `.npy` files written by NumPy

These files were written by NumPy 2.5.3 on Linux x86-64. The build tests in
[`../../build/npy.rs`](../../build/npy.rs) read them with `Array.load` and compare what
`Array.save` writes with them byte for byte, so the tests show that Baryon and NumPy share
`.npy` files (decision 0043) without Python running in the tests.

To record them again, run this in this directory:

```text
import numpy as np

grid = np.array([[0.0, 1.0, 2.0], [10.0, 11.0, 12.0]])
np.save("grid.npy", grid)
np.save("column.npy", grid[:, 1])
np.save("i16.npy", np.array([1, -2, 300], dtype=np.int16))
np.save("bool.npy", np.array([True, False, True]))
np.save("complex128.npy", np.array([1 + 2j, -0.5j]))
np.save("cube.npy", np.arange(24, dtype=np.uint8).reshape(2, 3, 4))
np.save("empty.npy", np.zeros((0, 3)))
np.save("fortran.npy", np.asfortranarray(grid))
np.save("big_endian.npy", grid.astype(">f8"))
with open("version_2.npy", "wb") as file:
    np.lib.format.write_array(file, grid, version=(2, 0))
```
