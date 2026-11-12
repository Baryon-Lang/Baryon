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
`saddle.svg`, `auto.svg`, `logaxes.svg` and `lognarrow.svg` are Baryon's own output, checked by
eye against matplotlib's drawing of the same data and stored so any change to how a plot is drawn
shows up as a difference. When a change to drawing is intended, write them again from the
programs in `plotting.rs`, `plotting_images.rs`, which draws `grid.svg`, `saddle.svg` and
`auto.svg`, and `plotting_log.rs`, which draws `logaxes.svg` and `lognarrow.svg`, and look at
them before committing.
