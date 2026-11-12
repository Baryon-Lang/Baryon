# FITS files written by AstroPy

These files were written by AstroPy 8.0.2 with NumPy 2.5.3 on Linux x86-64. The build tests in
[`../../build/fits.rs`](../../build/fits.rs) read them with `fits.getdata` and
`fits.getheader` and compare what `fits.writeto` writes with them byte for byte, so the tests
show that Baryon and AstroPy share FITS files (decision 0044) without Python running in the
tests.

- `image.fits`: a 3 by 4 `uint16` image, stored with `BZERO = 32768`, and four cards.
- `float32.fits`: a 2 by 3 `float32` image.
- `scaled.fits`: a 2 by 2 image stored as `int16` with `BSCALE = 0.5` and `BZERO = 10`.
- `extension.fits`: an empty primary HDU, then a 2 by 2 `int32` image in HDU 1.
- `edited.fits`: `image.fits` written again after setting `EXPTIME` and adding `FILTER`.
- `plain.fits`: `image.fits`'s pixels written as `float64` with no header.

To record them again, run this in this directory:

```text
import numpy as np
from astropy.io import fits

header = fits.Header()
header["OBJECT"] = ("M31", "target")
header["EXPTIME"] = (30.0, "seconds")
header["NCOMBINE"] = 3
header["DONE"] = True
image = np.arange(12, dtype=np.uint16).reshape(3, 4) * 1000
fits.writeto("image.fits", image, header)

fits.writeto("float32.fits", np.array([[0.5, -1.25, 2.0], [3.0, 1e-3, -7.5]], dtype=np.float32))

scaled = fits.PrimaryHDU(np.array([[1.0, 2.0], [3.5, 4.5]]))
scaled.scale("int16", bscale=0.5, bzero=10.0)
scaled.writeto("scaled.fits")

extension = fits.HDUList([
    fits.PrimaryHDU(),
    fits.ImageHDU(np.array([[1, -2], [3, -4]], dtype=np.int32), name="SCI"),
])
extension.writeto("extension.fits")

edited = fits.getheader("image.fits")
edited["EXPTIME"] = 31.5
edited["FILTER"] = ("V", "band")
fits.writeto("edited.fits", fits.getdata("image.fits"), edited)
fits.writeto("plain.fits", fits.getdata("image.fits").astype(np.float64))
```
