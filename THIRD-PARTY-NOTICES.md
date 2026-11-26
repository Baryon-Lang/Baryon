# Third-party notices

Baryon is licensed under the [Apache License 2.0](LICENSE) or the [MIT License](LICENSE-MIT), at
your option. This file lists every piece of third-party content the repository carries, as
opposed to a dependency `Cargo.toml` declares, with its license and the files that hold it. Each
license below permits the use made of it. [Rule 6](CONTRIBUTING.md#rule-6-few-dependencies) asks
that new content be listed here in the pull request that adds it.

## QUADPACK

The adaptive quadrature routines `dqagse`, `dqagpe`, `dqagie`, `dqk21`, `dqk15i`, `dqelg` and
`dqpsrt` of QUADPACK, by R. Piessens, E. de Doncker-Kapenga, C. Überhuber and D. Kahaner,
translated to Baryon step for step for `integrate.quad` (roadmap 3.19.4 and 3.19.5), with the
21-point and 15-point Gauss-Kronrod nodes and weights they hold. QUADPACK is in the public
domain, as Netlib distributes it, so no license applies.

| File | Holds |
| --- | --- |
| [`crates/driver/library/integrate_quadpack.byn`](crates/driver/library/integrate_quadpack.byn) | `dqk21` and `dqk15i` with their nodes and weights, `dqelg`, `dqpsrt` and the end of `dqagse`, `dqagpe` and `dqagie` |
| [`crates/driver/library/integrate_quad.byn`](crates/driver/library/integrate_quad.byn) | The bodies of `dqagse`, `dqagpe` and `dqagie` |

The source is the Fortran SciPy shipped under `scipy/integrate/quadpack/` until version 1.14.

## Apache parquet-testing

Ten Parquet files of the Apache Parquet project's
[`parquet-testing`](https://github.com/apache/parquet-testing) collection, copied unchanged
from its `data/` directory at commit `0e00127a431f83388d38eebb22cf644d06e86a15`. The tests of
`DataFrame.read_parquet` read them, as files other writers made (roadmap 3.14.6.3). They are
test data only: nothing of them is built into the compiler or into a program it compiles.

| File | Holds |
| --- | --- |
| [`crates/driver/tests/fixtures/parquet/apache/`](crates/driver/tests/fixtures/parquet/README.md#apache-files-of-other-writers) | `alltypes_plain.parquet`, `alltypes_plain.snappy.parquet`, `alltypes_dictionary.parquet`, `datapage_v2.snappy.parquet`, `delta_binary_packed.parquet`, `delta_byte_array.parquet`, `delta_encoding_optional_column.parquet`, `delta_length_byte_array.parquet`, `lz4_raw_compressed.parquet` and `rle_boolean_encoding.parquet`, and beside each a `.csv` of the values read from it |

The collection is licensed under the Apache License, Version 2.0, and carries no `NOTICE`
file. The license's text is this repository's own [`LICENSE`](LICENSE) file, word for word.
These are the only files here under that license alone: Baryon's own files are under it or
the MIT License, at the user's option.

## DejaVu Sans

The outlines, advances, kerning and character codes of DejaVu Sans, the font matplotlib draws
text in, for 135 characters: printable ASCII, the Greek letters, `×`, `−`, `°`, `±`, `µ` and `·`.
`byplot` draws text in PNG files and embeds the font in PDF files from them (decision
[0046](docs/decisions/0046-plotting.md)).

| File | Holds |
| --- | --- |
| [`crates/driver/library/dejavu_sans.byn`](crates/driver/library/dejavu_sans.byn) | The outlines and the notice below, written from matplotlib's copy of `DejaVuSans.ttf` by the script in the [plotting fixtures' README](crates/driver/tests/fixtures/plotting/README.md#dejavu-sans) |

Its notice, as the font states it:

```text
Fonts are (c) Bitstream (see below). DejaVu changes are in public domain. Glyphs imported from
Arev fonts are (c) Tavmjung Bah (see below)

Bitstream Vera Fonts Copyright
------------------------------

Copyright (c) 2003 by Bitstream, Inc. All Rights Reserved. Bitstream Vera is
a trademark of Bitstream, Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy
of the fonts accompanying this license ("Fonts") and associated
documentation files (the "Font Software"), to reproduce and distribute the
Font Software, including without limitation the rights to use, copy, merge,
publish, distribute, and/or sell copies of the Font Software, and to permit
persons to whom the Font Software is furnished to do so, subject to the
following conditions:

The above copyright and trademark notices and this permission notice shall
be included in all copies of one or more of the Font Software typefaces.

The Font Software may be modified, altered, or added to, and in particular
the designs of glyphs or characters in the Fonts may be modified and
additional glyphs or characters may be added to the Fonts, only if the fonts
are renamed to names not containing either the words "Bitstream" or the word
"Vera".

This License becomes null and void to the extent applicable to Fonts or Font
Software that has been modified and is distributed under the "Bitstream
Vera" names.

The Font Software may be sold as part of a larger software package but no
copy of one or more of the Font Software typefaces may be sold by itself.

THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF COPYRIGHT, PATENT,
TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL BITSTREAM OR THE GNOME
FOUNDATION BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, INCLUDING
ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL DAMAGES,
WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF
THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM OTHER DEALINGS IN THE
FONT SOFTWARE.

Except as contained in this notice, the names of Gnome, the Gnome
Foundation, and Bitstream Inc., shall not be used in advertising or
otherwise to promote the sale, use or other dealings in this Font Software
without prior written authorization from the Gnome Foundation or Bitstream
Inc., respectively. For further information, contact: fonts at gnome dot
org.

Arev Fonts Copyright
------------------------------

Copyright (c) 2006 by Tavmjong Bah. All Rights Reserved.

Permission is hereby granted, free of charge, to any person obtaining
a copy of the fonts accompanying this license ("Fonts") and
associated documentation files (the "Font Software"), to reproduce
and distribute the modifications to the Bitstream Vera Font Software,
including without limitation the rights to use, copy, merge, publish,
distribute, and/or sell copies of the Font Software, and to permit
persons to whom the Font Software is furnished to do so, subject to
the following conditions:

The above copyright and trademark notices and this permission notice
shall be included in all copies of one or more of the Font Software
typefaces.

The Font Software may be modified, altered, or added to, and in
particular the designs of glyphs or characters in the Fonts may be
modified and additional glyphs or characters may be added to the
Fonts, only if the fonts are renamed to names not containing either
the words "Tavmjong Bah" or the word "Arev".

This License becomes null and void to the extent applicable to Fonts
or Font Software that has been modified and is distributed under the
"Tavmjong Bah Arev" names.

The Font Software may be sold as part of a larger software package but
no copy of one or more of the Font Software typefaces may be sold by
itself.

THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL
TAVMJONG BAH BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.

Except as contained in this notice, the name of Tavmjong Bah shall not
be used in advertising or otherwise to promote the sale, use or other
dealings in this Font Software without prior written authorization
from Tavmjong Bah. For further information, contact: tavmjong @ free
. fr.
```
