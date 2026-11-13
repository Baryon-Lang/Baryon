# Ticks recorded from matplotlib, and reference plots

`ranges.txt` holds 310 axis ranges, one `vmin vmax` pair a line: ten written by hand, from
`0 10` to ranges near a large offset, and 300 drawn at random over magnitudes from 1e-6 to 1e8.
`matplotlib_ticks.txt` holds, for each range, two lines that matplotlib 3.11.2 wrote on Linux
x86-64: the positions its `MaxNLocator(nbins=9, steps=[1, 2, 2.5, 5, 10])` gives, as Python's
`repr`, and the labels its `ScalarFormatter` writes for them on an axis showing that range,
joined by `|`, then a space and the offset text. The build tests in
[`../../build/plotting.rs`](../../build/plotting.rs) compare `ticker`'s results with them line
for line, so the tests show that Baryon places and labels ticks as matplotlib does
(decision 0046) without Python running in the tests.

To record them again, run this in this directory:

```text
import matplotlib
matplotlib.use("svg")
import matplotlib.pyplot as plt
from matplotlib.ticker import MaxNLocator, ScalarFormatter

locator = MaxNLocator(nbins=9, steps=[1, 2, 2.5, 5, 10])
fig, ax = plt.subplots()
lines = []
for line in open("ranges.txt"):
    low, high = map(float, line.split())
    ticks = locator.tick_values(low, high)
    lines.append(" ".join(repr(float(tick)) for tick in ticks))
    ax.set_xlim(low, high)
    formatter = ScalarFormatter()
    formatter.set_axis(ax.xaxis)
    lines.append("|".join(formatter.format_ticks(ticks)) + " " + formatter.get_offset())
open("matplotlib_ticks.txt", "w").write("\n".join(lines) + "\n")
```

`log_ranges.txt` holds 176 positive ranges for logarithmic axes: eleven written by hand, from
a tenth of a decade to sixty decades, then every range of `ranges.txt` above 0.
`matplotlib_log_ticks.txt` holds five lines for each: the ticks matplotlib 3.11.2's
`LogLocator(subs=(1.0,), numticks=9)` gives, the labels its `LogFormatterSciNotation` writes for
them on an axis showing that range, joined by `|`, the same two for `LogLocator(subs="auto",
numticks=9)`, the minor ticks, and the ticks of `LogLocator(subs=(1.0,), numticks=3)`. The build
tests in [`../../build/plotting_log.rs`](../../build/plotting_log.rs) compare `ticker`'s results
with them. It was written with:

```text
import matplotlib
matplotlib.use("svg")
import matplotlib.pyplot as plt
from matplotlib.ticker import LogFormatterSciNotation, LogLocator

fig, ax = plt.subplots()
ax.set_xscale("log")
lines = []
for line in open("log_ranges.txt"):
    low, high = map(float, line.split())
    ax.set_xlim(low, high)
    formatter = LogFormatterSciNotation()
    formatter.set_axis(ax.xaxis)
    for subs in [(1.0,), "auto"]:
        ticks = LogLocator(subs=subs, numticks=9).tick_values(low, high)
        lines.append(" ".join(repr(float(tick)) for tick in ticks))
        lines.append("|".join(formatter.format_ticks(ticks)))
    ticks = LogLocator(subs=(1.0,), numticks=3).tick_values(low, high)
    lines.append(" ".join(repr(float(tick)) for tick in ticks))
open("matplotlib_log_ticks.txt", "w").write("\n".join(lines) + "\n")
```

`matplotlib_colormaps.txt` holds, for each colormap `cm` has and each one reversed, the colors
matplotlib 3.11.2's `colormaps[name](x, bytes=True)` gives at eleven values of `x`, from -0.5
to 1.5 and NaN, as red, green, blue and alpha bytes. The tests in
[`../../build/plotting_images.rs`](../../build/plotting_images.rs) draw each colormap over those
values with `imshow` and compare the pixels of the PNG inside the SVG with it. It was written
with:

```text
import matplotlib as m
names = ["viridis", "plasma", "inferno", "magma", "cividis", "gray", "Greys", "coolwarm", "RdBu"]
xs = [-0.5, 0.0, 0.001, 0.1, 0.25, 0.5, 0.7, 0.999, 1.0, 1.5, float("nan")]
with open("matplotlib_colormaps.txt", "w") as f:
    for name in names + [n + "_r" for n in names]:
        colors = [m.colormaps[name](x, bytes=True) for x in xs]
        f.write(name + " " + " ".join(",".join(str(int(v)) for v in c) for c in colors) + "\n")
```

`polynomials.svg`, `offset.svg`, `shapes.svg`, `markers.svg`, `histogram.svg`, `grid.svg`,
`saddle.svg`, `auto.svg`, `logaxes.svg`, `lognarrow.svg`, `figures.svg`, `contours.svg` and
`saddles.svg` are Baryon's own
output, checked by eye against matplotlib's drawing of the same data and stored so any change to
how a plot is drawn shows up as a difference. When a change to drawing is intended, write them
again from the programs in `plotting.rs`, `plotting_images.rs`, which draws `grid.svg`,
`saddle.svg` and `auto.svg`, `plotting_log.rs`, which draws `logaxes.svg` and `lognarrow.svg`,
`plotting_figures.rs`, which draws `figures.svg`, and `plotting_contour.rs`, which draws
`contours.svg` and `saddles.svg`, and look at them before committing.
`figures.svg` was also compared with matplotlib 3.11.2's SVG of the same figure, drawn with
`font.family` set to DejaVu Sans: its text is placed within a thousandth of a point of
matplotlib's, apart from labels whose width is estimated. `contours.svg` and `saddles.svg` were
compared with matplotlib's drawings of the same grids: the lines and bands fall in the same
places, the colors are the same bytes, and only where a dash starts differs. `polynomials.svg`
and `shapes.svg` were written again when dashed lines took matplotlib's `"butt"` dash cap.

`matplotlib_contour_ticks.txt` holds, for each range of `ranges.txt`, the levels matplotlib
3.11.2's `MaxNLocator(8)` gives with its default steps, as contour levels are chosen before
they are trimmed to the data. [`../../build/plotting_contour.rs`](../../build/plotting_contour.rs)
compares `ticker`'s with them. It was written with:

```text
from matplotlib.ticker import MaxNLocator

locator = MaxNLocator(8)
lines = []
for line in open("ranges.txt"):
    low, high = map(float, line.split())
    lines.append(" ".join(repr(float(t)) for t in locator.tick_values(low, high)))
open("matplotlib_contour_ticks.txt", "w").write("\n".join(lines) + "\n")
```

## PNG images

`waves.png` and `grid.png` are Baryon's own PNG output, from the programs in
[`../../build/plotting_png.rs`](../../build/plotting_png.rs), checked by eye against
matplotlib 3.11.2's PNG of the same plots: the same lines, dashes, markers, bands and colors in
the same places, with text in DejaVu Sans where matplotlib 3.11 draws its newer default font. The
tests read both files back and allow a color value to differ by one level, so the last bit of
`sin` and `cos` may differ between platforms. When a change to drawing is intended, write them
again from those programs and look at them before committing.

## DejaVu Sans

`crates/driver/library/dejavu_sans.byn` is written from matplotlib's own copy of
`DejaVuSans.ttf` by this script, run with `fontTools` installed, the font's path and the output
path as its arguments:

```text
import sys
import textwrap
from fontTools.ttLib import TTFont

font = TTFont(sys.argv[1])
cmap = font.getBestCmap()
glyf = font["glyf"]
hmtx = font["hmtx"]
chars = (
    [chr(c) for c in range(32, 127)] + list("×−°±µ·")
    + [chr(c) for c in range(0x3B1, 0x3CA) if c != 0x3C2] + list("ΓΔΘΛΞΠΣΦΨΩ")
)
B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"


def two(value, offset):
    value += offset
    assert 0 <= value < 4096
    return B64[value >> 6] + B64[value & 63]


def glyph(name):
    g = glyf[name]
    out = two(hmtx[name][0], 0)
    if g.numberOfContours == 0:
        return out
    coords, ends, flags = g.getCoordinates(glyf)
    start, parts = 0, []
    for end in ends:
        parts.append("".join(
            ("" if flags[k] & 1 else "~") + two(coords[k][0], 1024) + two(coords[k][1], 1024)
            for k in range(start, end + 1)
        ))
        start = end + 1
    return out + ".".join(parts)


names = [".notdef"] + [cmap[ord(c)] for c in chars]
index = {name: k for k, name in enumerate(names)}
data = ",".join(glyph(name) for name in names)
kerning = "".join(
    two(index[a], 0) + two(index[b], 0) + two(v, 2048)
    for (a, b), v in sorted(font["kern"].kernTables[0].kernTable.items())
    if a in index and b in index and a != ".notdef"
)
```

The script then writes `data`, `kerning`, the characters and the font's license notice, its
`name` record 13 wrapped at 96 columns, into the file's functions and docstring, with each long
string cut into lines of at most 88 characters joined by `+`.
