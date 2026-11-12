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

`polynomials.svg`, `offset.svg`, `shapes.svg`, `markers.svg` and `histogram.svg` are Baryon's
own output, checked by eye against matplotlib's drawing of the same data and stored so any change
to how a plot is drawn shows up as a difference. When a change to drawing is intended, write them
again from the programs in `plotting.rs` and look at them before committing.
