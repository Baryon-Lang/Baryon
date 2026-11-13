# Plotting

The `byplot` module draws plots with matplotlib's names and matplotlib's default look, written in
Baryon and saved as SVG files that any browser, VS Code or a LaTeX toolchain can show, or as PNG
images ([decision 0046](../decisions/0046-plotting.md)). The `ticker` module beside it places and
labels the ticks, as `matplotlib.ticker` does. These two are the modules a program imports.
`byplot` draws through smaller files of the standard library, named as matplotlib names its own:
`colors` reads color names, `cm` holds the colormaps, `artist` holds what an axes draws,
`contour` finds contour lines and bands,
`transforms` finds the ranges it shows, `figure` lays a grid of axes out and draws them, `axis`
draws an axis's ticks and labels, `legend`, `image` and `colorbar` draw a legend, an image and a
colorbar, `backend_bases` lists the calls every renderer takes, `backend_svg` writes the SVG
file, `backend_agg` draws the PNG image through `raster`, which turns shapes into pixels, and
`font`, which reads DejaVu Sans's outlines from `dejavu_sans`, and `png` writes PNG files. They
are internal, so a program's own `colors.byn` or `image.byn` is never hidden by them.

This chapter covers what builds today, roadmap 3.15.1 to 3.15.6: line plots, scatter plots,
bars, histograms, error bars, filled areas, text, markers, legends, grid lines, images and
colorbars, contour plots, titles, axis labels, axis ranges, logarithmic axes, grids of plots, and
SVG and PNG output. PDF files follow in roadmap 3.15.7.

## A first plot

A damped oscillator's position and velocity over ten seconds:

```python
from math import cos, exp, sin
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    n = 200
    t = Array.zeros(n)
    x = Array.zeros(n)
    v = Array.zeros(n)
    for i in range(n):
        t[i] = 10.0 * f64(i) / f64(n - 1)
        x[i] = exp(-0.2 * t[i]) * cos(2.0 * t[i])
        v[i] = exp(-0.2 * t[i]) * sin(2.0 * t[i])
    ax = plt.axes()
    ax.plot(t, x, label="position")
    ax.plot(t, v, linestyle="--", label="velocity")
    ax.set_xlabel("time (s)")
    ax.set_ylabel("amplitude (m)")
    ax.set_title("Damped oscillator")
    ax.savefig("oscillator.svg")
```

`plt.axes()` makes an empty `Axes`: the rectangle the data are drawn in, with an x axis below it
and a y axis to its left. Each method changes the axes in place, and `savefig` writes a 6.4 by
4.8 inch figure holding them. The first line is drawn in matplotlib's blue and the second in its
orange, the first two colors of its `tab10` cycle; the ticks fall at 0, 2, 4, ... and at 0.25
steps, where matplotlib puts them, and negative labels use the minus sign `−`, as matplotlib's do.

## Coming from matplotlib

A matplotlib script changes in two places:

| matplotlib | Baryon | Why |
| --- | --- | --- |
| `import matplotlib.pyplot as plt` | `import byplot as plt` | Libraries are named without their package, as `linalg` is for `numpy.linalg`, and Baryon's own names say `by` where Python's say `py` ([decision 0046](../decisions/0046-plotting.md)) |
| `fig, ax = plt.subplots()` | `ax = plt.axes()` | Baryon values have one owner, so `fig` and `ax` cannot share a figure: drawing on `ax` would not change `fig` |
| `fig.savefig("p.svg")` | `ax.savefig("p.svg")` | The axes are saved in a figure of their own |
| `fig, axs = plt.subplots(2, 2)`, `axs[1, 0].plot(t, x)` | `fig = plt.subplots(2, 2)`, `fig.axes[2].plot(t, x)` | The figure owns its axes, so they are reached through it, row by row, as matplotlib's `fig.axes` lists them |
| `ax.errorbar(x, y, yerr=None)`, `ax.fill_between(x, y1, 0.5)` | leave `yerr` out; `ax.fill_between(x, y1, Array.fill(len(x), 0.5))` | An array argument that may be absent is an empty array by default, and the curves are arrays |
| `im = ax.imshow(z)`, `fig.colorbar(im)` | `ax.imshow(z)`, `ax.colorbar()` | The colorbar belongs to the axes and shows its last image, as there is no `fig` to ask |
| `plt.plot(t, x)`, `plt.savefig(...)` | not provided | These draw on a hidden current figure, a module-level variable, and Baryon has none: a parallel loop would race on it |

Everything else keeps matplotlib's names: the methods, their keyword arguments and the color and
line style names. A script that uses `plt.plot` fails to compile with the missing name, so a
moved script is never drawn wrong in silence.

## `Axes`

| Method | Does | matplotlib |
| --- | --- | --- |
| `ax.plot(x, y, color="", linewidth=1.5, linestyle="-", marker="", markersize=6.0, label="")` | Draws the points `(x[i], y[i])` joined by straight lines, with a marker at each when `marker` is given | `Axes.plot(x, y, ...)` |
| `ax.scatter(x, y, s=36.0, color="", marker="o", label="")` | Draws a marker at each point; `s` is its area in points squared | `Axes.scatter` |
| `ax.bar(x, height, width=0.8, color="", label="")` | Draws a bar from 0 to `height[i]` centred on each `x[i]` | `Axes.bar` |
| `counts, edges = ax.hist(x, bins=10, color="", label="")` | Draws the histogram of `x` in `bins` equal bins and returns the counts and the `bins + 1` edges | `Axes.hist`, which also returns the bars |
| `ax.errorbar(x, y, yerr=Array.zeros(0), xerr=Array.zeros(0), fmt="", color="", capsize=0.0, label="")` | Draws the points with an error bar of `yerr[i]` up and down and `xerr[i]` left and right of each; an empty array draws no bars | `Axes.errorbar`, with errors of the data's length |
| `ax.fill_between(x, y1, y2=Array.zeros(0), color="", alpha=1.0, label="")` | Fills the area between the curves `y1` and `y2`, or down to 0 when `y2` is left out | `Axes.fill_between` |
| `ax.text(x, y, s, fontsize=10.0, ha="left", va="baseline")` | Writes `s` at the data point `(x, y)` | `Axes.text` |
| `ax.contour(x, y, Z, levels=Array.zeros(0), colors="", cmap="viridis", linewidths=1.5)` | Draws the lines where the `Array[f64, 2]` `Z` crosses each level | `Axes.contour(x, y, Z, ...)` |
| `ax.contourf(x, y, Z, levels=Array.zeros(0), cmap="viridis", alpha=1.0)` | Fills the bands between neighbouring levels of `Z` | `Axes.contourf(x, y, Z, ...)` |
| `ax.legend(loc="best")` | Draws a legend of every labelled line, set of markers, set of bars, set of error bars and filled area | `Axes.legend` |
| `ax.grid(visible=True)` | Draws light gray lines across the axes at every tick, or stops drawing them | `Axes.grid` |
| `ax.imshow(X, cmap="viridis", vmin=None, vmax=None, origin="upper", aspect="equal", extent=None)` | Draws the `Array[f64, 2]` `X` as an image, one colored cell per element | `Axes.imshow` |
| `ax.colorbar(label="")` | Draws a colorbar of the last image on the right of the axes | `Figure.colorbar(im, ax=ax, label=...)` |
| `ax.set_title(label)` | Sets the title, centred above the axes, at 12 points | `Axes.set_title` |
| `ax.set_xlabel(xlabel)`, `ax.set_ylabel(ylabel)` | Sets the label under the x axis, or left of the y axis | `Axes.set_xlabel`, `Axes.set_ylabel` |
| `ax.set_xlim(left, right)`, `ax.set_ylim(bottom, top)` | Fixes the range shown; a first value larger than the second turns the axis around | `Axes.set_xlim`, `Axes.set_ylim` |
| `ax.set_xscale(value)`, `ax.set_yscale(value)` | Lays an axis out evenly, `"linear"`, or by the logarithm of the value, `"log"` | `Axes.set_xscale`, `Axes.set_yscale` |
| `ax.savefig(fname)` | Writes a figure holding the axes; the extension picks the format, `.svg` or [`.png`](#png-files) | `Figure.savefig` |

`x` and `y` are `Array[f64, 1]`s of the same length; `plot` copies them, so they can change
afterwards. Lists and integer arrays come with generic functions over the element type: until
then, `Array([1.0, 2.0])` makes an array of a list.

A point with a NaN or infinite coordinate is left out and breaks the line there, as matplotlib
breaks it, so missing measurements show as gaps; `scatter` leaves such a point out.

`hist` counts as `stats.histogram` does, with NumPy's edges and NumPy's rule that the last bin
holds its right edge; `counts` is an `Array[f64, 1]`, as matplotlib's is, so it can be scaled
or plotted straight away. Bars and histograms start at 0: the y axis reaches 0 exactly, with no
margin below it, as matplotlib's does.

```python
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    data = Array.zeros(200)
    for i in range(200):
        data[i] = 0.25 * f64((i * i) % 17)
    ax = plt.axes()
    counts, edges = ax.hist(data, bins=8, label="counts")
    print(counts)  # [35. 24. 23.  0. 47.  0. 24. 47.]
    ax.legend(loc="upper left")
    ax.savefig("histogram.svg")
```

### Images

`imshow` draws `X[i, j]` as a cell centred on x = `j` and y = `i`, colored by its colormap from
`vmin`, the first color, to `vmax`, the last; they default to the smallest and largest finite
element, and a value outside them takes the end color, as matplotlib's does. A NaN is left
transparent. Row 0 is at the top, so the y axis runs downwards, until `origin="lower"` puts it
at the bottom; `extent=(left, right, bottom, top)` stretches the image over that rectangle of
data instead. With `aspect="equal"`, the default, a unit of x and of y have the same length, so
the axes narrow about their centre as matplotlib's do; `aspect="auto"` lets the image fill them.

`cmap` is one of matplotlib's colormaps: `"viridis"`, `"plasma"`, `"inferno"`, `"magma"`,
`"cividis"`, `"gray"`, `"Greys"`, `"coolwarm"` or `"RdBu"`, or any of them with `"_r"` for the
same colors reversed. Each is matplotlib's table of 256 colors, and a value picks its color as
matplotlib picks it, so an image's pixels are matplotlib's bytes; the tests check every map
against colors recorded from matplotlib.

`colorbar` takes 20% of the axes' width on the right, as matplotlib's default does, and draws
the colormap from `vmin` to `vmax` with ticks placed and labelled by `ticker`.

```python
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    z = Array.zeros((24, 40))
    for i in range(24):
        for j in range(40):
            x = 0.125 * f64(j - 20)
            y = 0.125 * f64(i - 12)
            z[i, j] = x * x - y * y
    ax = plt.axes()
    ax.imshow(z, cmap="RdBu_r", vmin=-4.0, vmax=4.0, origin="lower")
    ax.colorbar(label="height (m)")
    ax.savefig("saddle.svg")
```

The image goes into the SVG file as a PNG with one pixel a cell. When a cell spans three
pixels or more each way, at matplotlib's 100 dots an inch, the viewer is told to keep cells
sharp, as matplotlib's `"nearest"` interpolation does; a smaller image is left to the viewer
to smooth, which is close to matplotlib's antialiasing but not the same pixels.

### Error bars, filled areas and text

`errorbar` draws its bars first, 1.5 points wide, then the points over them, all in one color,
the next of the line cycle unless `color` is given, as matplotlib does. `fmt` says how the points
are drawn: `""` or `"-"` joins them with a line, a marker such as `"o"` marks each one, the
marker then `"-"`, such as `"o-"`, does both, and `"none"` draws the bars alone. `capsize` puts
a cap across each end of each bar, `capsize` points long on each side; the default, 0, draws
none, as matplotlib's default does. The range shown covers the ends of the bars. A legend shows
the bars, caps and points together.

`fill_between` fills the polygon that runs along `y1` and back along `y2`, with no edge, in the
next color of the cycle bars take from, as matplotlib's does; `alpha` from 0 to 1 sets its
opacity. A NaN or infinite value in `x`, `y1` or `y2` splits the area there. Fills lie under the
grid and lines, as bars do.

`text` writes at a point in data coordinates. `ha` puts the text's `"left"` side, the default,
its `"center"` or its `"right"` side at `x`; `va` puts its `"baseline"`, the default, the
`"bottom"` or `"top"` of its line or its `"center"` at `y`, measured with matplotlib's line box
for DejaVu Sans. Text is not clipped to the axes and does not change the range shown, as in
matplotlib.

```python
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    t = Array([1.0, 2.0, 3.0, 4.0, 5.0])
    rate = Array([2.1, 3.9, 6.2, 7.8, 10.1])
    spread = Array([0.3, 0.4, 0.4, 0.6, 0.7])
    ax = plt.axes()
    ax.fill_between(t, 2.0 * t - 0.5, 2.0 * t + 0.5, alpha=0.2, label="model")
    ax.errorbar(t, rate, yerr=spread, fmt="o", capsize=3.0, label="measured")
    ax.text(1.2, 9.0, "rate = 2t", fontsize=12.0)
    ax.legend(loc="lower right")
    ax.savefig("rate.svg")
```

### Contour plots

`contour` draws the lines where `Z` crosses each level, and `contourf` fills the bands between
neighbouring levels. `Z[i, j]` is the value at `(x[j], y[i])`, as matplotlib reads it with 1-D
`x` and `y`, so `x` has one value per column and `y` one per row. With `levels` left out, about
seven levels are chosen as matplotlib chooses them, at round values spanning the data; give
`levels` as a rising array to pick them. Lines and bands are colored by `cmap` over the levels,
in matplotlib's colors to the byte; `colors="k"` draws every line in one color instead, with the
negative levels dashed, as matplotlib draws them. A band holds its top level, and the lowest
band its bottom level too. The range shown fits the grid exactly, with no margin.

The lines are found by marching squares: each cell between four neighbouring points is crossed
where its edges cross a level, by straight-line interpolation along the edge, and a cell whose
corners alternate about a level is split by the mean of its corners, as matplotlib's default
algorithm splits it. A cell with a NaN corner is left out, where matplotlib also fills the
corner triangles beside it.

```python
from math import exp
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    x = Array.zeros(41)
    y = Array.zeros(31)
    for j in range(41):
        x[j] = -2.0 + 0.1 * f64(j)
    for i in range(31):
        y[i] = -1.5 + 0.1 * f64(i)
    potential = Array.zeros((31, 41))
    for i in range(31):
        for j in range(41):
            potential[i, j] = x[j] * exp(-x[j] * x[j] - y[i] * y[i])
    ax = plt.axes()
    ax.contourf(x, y, potential, cmap="RdBu_r")
    ax.contour(x, y, potential, colors="k")
    ax.set_title("Dipole potential")
    ax.savefig("dipole.svg")
```

`levels` takes an array where matplotlib also takes a count: a count does not build yet. Contour
labels (`clabel`), a colorbar of contours, 2-D `X` and `Y` grids, and `extend` are not offered.
A file holds one path a level: about 15 bytes a point of a line, and up to 60 a cell a filled
band covers in part; whole cells in a row are merged into one rectangle.

### Grids of plots

`plt.subplots(nrows, ncols)` makes a `Figure` holding a grid of empty axes, laid out with
matplotlib's default subplot parameters: the grid fills the same box one axes fills, with gaps of
a fifth of an axes between them. `fig.axes` lists them row by row, so in a 2 by 3 grid
`fig.axes[0]` is the top left one and `fig.axes[3]` the first of the second row. Each is an
ordinary `Axes`, with its own color cycles, ranges and legend, and `fig.savefig` writes them all
in one file.

```python
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    t = Array.zeros(50)
    for i in range(50):
        t[i] = 0.1 * f64(i)
    fig = plt.subplots(2, 1)
    fig.axes[0].plot(t, t * t)
    fig.axes[0].set_ylabel("position (m)")
    fig.axes[1].plot(t, 2.0 * t)
    fig.axes[1].set_ylabel("velocity (m/s)")
    fig.axes[1].set_xlabel("time (s)")
    fig.savefig("motion.svg")
```

Labels and titles of neighbouring axes can overlap when the grid is dense, as they do in
matplotlib without `constrained_layout`, which is not offered yet. `subplots` panics when either
count is below 1, where matplotlib raises `ValueError`.

### Markers

`marker` is one of matplotlib's: `"o"` a circle, `"."` a small circle, `"s"` a square, `"^"`
and `"v"` triangles pointing up and down, `"D"` a diamond, `"+"` and `"x"` crosses, and `"_"`
and `"|"` short lines across and up. In
`plot` it is `markersize` points across; a `linestyle` of `""` or `"None"` leaves the line out,
so only the markers show. Each marker is written once in the file and placed at every point, so
a point costs about 60 bytes.

### Legends

A legend lists the artists with a `label`, in the order they were drawn, each with a short
sample of its line, marker, bar or filled area; an `errorbar`'s sample is its point with a bar
through it, and its caps. `loc` is `"best"`, or one of matplotlib's places:
`"upper right"`, `"upper left"`, `"lower left"`, `"lower right"`, `"right"`, `"center left"`,
`"center right"`, `"lower center"`, `"upper center"` and `"center"`. `"best"` tries the ten
places matplotlib tries, in its order, and takes the first that covers the fewest data points,
bars and line segments, as matplotlib does. Calling `legend` before or after drawing gives the
same legend, since it is laid out when the plot is saved.

### Colors and line styles

`color` takes the names matplotlib takes:

| Name | Color |
| --- | --- |
| `"#rrggbb"` | That color, in hexadecimal |
| `"C0"` to `"C9"` | The colors of the cycle, in order |
| `"tab:blue"`, `"tab:orange"`, `"tab:green"`, `"tab:red"`, `"tab:purple"`, `"tab:brown"`, `"tab:pink"`, `"tab:gray"`, `"tab:olive"`, `"tab:cyan"` | The same colors by name |
| `"b"`, `"g"`, `"r"`, `"c"`, `"m"`, `"y"`, `"k"`, `"w"` | Blue, green, red, cyan, magenta, yellow, black and white, as matplotlib defines the letters |

Left out, or `""`, each line takes the next color of the cycle, as matplotlib's `color=None`
does. Lines have one cycle, and `scatter`, `bar` and `hist` share another, as in matplotlib: a
plot's first line and its first bars are both blue. A borrowed `str | None` parameter does not
build yet, so `""` stands for "none" here.

`linestyle` is `"-"` or `"solid"`, `"--"` or `"dashed"`, `":"` or `"dotted"`, and `"-."` or
`"dashdot"`. Dashes are matplotlib's, measured in line widths, so a thicker line has longer
dashes.

`byplot` turns each name into the `"#rrggbb"` form an SVG file holds, as matplotlib's
`colors.to_hex` does.

### Ranges and ticks

Each axis shows the finite data with a margin of 5% of its spread on each side, as matplotlib's
default, until `set_xlim` or `set_ylim` fixes it. With no data an axis shows 0 to 1, and a line
whose values are all equal is widened by 5% of its value. Fixed limits too close to tell apart
are widened by 5% each way, as matplotlib widens them, and a NaN or infinite limit panics with
matplotlib's message, `Axis limits cannot be NaN or Inf`.

Ticks fall at multiples of a step of 1, 2, 2.5 or 5 times a power of ten, chosen so at most 9
intervals fit, which is what matplotlib's `AutoLocator` chooses at the default figure size. Their
labels share one number of decimals, the fewest that tell them apart. When the ticks are large,
small or close together, part of each number moves to the end of the axis, as matplotlib moves
it: the labels of ticks from 1000.00000002 to 1000.00000008 read `0.2` to `0.8`, and the y axis
says `1e−7+1e3` above its top end, meaning "times 10⁻⁷, plus 1000".

### Logarithmic axes

`ax.set_xscale("log")` or `ax.set_yscale("log")` lays an axis out by the logarithm of the value,
base 10, as matplotlib's `"log"` scale does. The range follows the data above 0, with a margin of
5% of its logarithm on each side; with no such data it is 1 to 10. Points at or below 0 are left
out, so a line breaks there. Ticks fall at powers of ten, every second or third one when many
decades show, labelled `10⁻²`, `10⁰`, `10³` with the exponent raised; shorter minor ticks mark 2
to 9 times each power when fewer than ten decades show. Inside one decade the minor ticks carry
the labels instead, `2×10⁰`, `3×10⁰`, as matplotlib's do.

```python
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    frequency = Array.zeros(41)
    for i in range(41):
        frequency[i] = 10.0 ** (0.1 * f64(i))
    gain = 1.0 / (1.0 + frequency * frequency / 100.0)
    ax = plt.axes()
    ax.plot(frequency, gain)
    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_xlabel("frequency (Hz)")
    ax.savefig("response.svg")
```

`set_xscale` panics on any scale but `"linear"` and `"log"`, where matplotlib raises
`ValueError`; `"symlog"` and `"logit"` are not offered.

## `ticker`

| Item | Does | matplotlib |
| --- | --- | --- |
| `MaxNLocator(nbins, steps)` | Places ticks at nice steps: `steps` rise from 1 to 10, and at most `nbins` intervals fit | `ticker.MaxNLocator(nbins=..., steps=...)` |
| `locator.tick_values(vmin, vmax)` | The positions for the range, as a `list[f64]`; the first and last can fall just outside it | `MaxNLocator.tick_values` |
| `ScalarFormatter(vmin, vmax)` | Labels ticks on an axis showing `vmin` to `vmax` | `ticker.ScalarFormatter()`, which reads the range from its axis |
| `formatter.format_ticks(values)` | The label of each tick, as a `list[str]` | `ScalarFormatter.format_ticks` |
| `formatter.get_offset(values)` | The offset text for those ticks, such as `1e−7+1e3`, or `""` | `ScalarFormatter.get_offset` |
| `LogLocator(subs, numticks)` | Places ticks on a logarithmic axis: at powers of ten with `subs` `[1.0]`, at those multiples of each power otherwise, and at 2 to 9 times each with `[]`; at most `numticks` decades are ticked | `ticker.LogLocator(subs=..., numticks=...)`, `[]` for `subs="auto"` |
| `LogFormatterSciNotation(vmin, vmax)` | Labels ticks on a logarithmic axis showing `vmin` to `vmax`, in matplotlib's mathtext: `$\mathdefault{10^{3}}$`, or `""` for a tick left unlabelled | `ticker.LogFormatterSciNotation()` |

```python
from ticker import MaxNLocator, ScalarFormatter


def main():
    locator = MaxNLocator(9, [1.0, 2.0, 2.5, 5.0, 10.0])
    ticks = locator.tick_values(-0.8, 1.1)
    print(ticks)  # [-1.0, -0.75, -0.5, -0.25, 0.0, 0.25, 0.5, 0.75, 1.0, 1.25]
    print(ScalarFormatter(-0.8, 1.1).format_ticks(ticks))
```

The positions are matplotlib's to the last bit, and the labels and offsets its exact text: the
tests compare both with what matplotlib 3.11 gives for 310 ranges, from `0 10` to ranges a
millionth wide near `-77380925.9`. Struct fields have no defaults, so both items are built with
every field, where matplotlib's have keyword arguments with defaults.

On a logarithmic axis, `LogLocator` ticks every power of ten, or every second or third when more
than `numticks` decades show, and puts minor ticks at 2 to 9 times each power when fewer than ten
do. `LogFormatterSciNotation` labels only the powers when the axis spans more than one decade,
adds 2, 3, 4 and 6 times a power when it spans more than 0.4 of one, and labels every minor tick
in a narrower range, as matplotlib does. The tests compare both with matplotlib 3.11 for 176
ranges, from a tenth of a decade to sixty decades. A tick can differ from matplotlib's in its last
bit: NumPy raises 10 to a power with a vectorized `pow` on some processors that is not always
correctly rounded, so matplotlib's own ticks differ between machines. Only base 10 is offered.

## The files under `byplot`

`byplot` draws through these internal files of the standard library. A program cannot import
them; this table is for those working on the library, and on the PNG and PDF backends to come.

| Item | Does | matplotlib |
| --- | --- | --- |
| `XAxis(vmin, vmax, left, right, bottom, log)` | The axis along the bottom of an axes, laid from `left` to `right` points at `bottom`, by the logarithm of the value when `log` is set | `axis.XAxis` |
| `YAxis(vmin, vmax, bottom, top, left, log)` | The axis along the left of an axes | `axis.YAxis` |
| `axis.get_majorticklocs()`, `axis.get_minorticklocs()` | The tick positions, with as many intervals as fit the axis's length; minor ticks only on a logarithmic axis | `Axis.get_majorticklocs`, `Axis.get_minorticklocs` |
| `axis.draw(renderer)` | Draws the ticks, minor ticks shorter and thinner, their labels and the offset text; returns where the axis label goes | `Axis.draw` |
| `RendererBase` | The trait every renderer has: the drawing calls below and `get_text_width_height_descent`. The drawing functions are generic over it, `[R: RendererBase]`, so `byplot` lays a figure out once for every format | `backend_bases.RendererBase` |
| `RendererSVG(width, height, [], 0)` | An empty SVG drawing, sizes in points, with no clip rectangle yet; `renderer.set_clip(x, y, width, height)` sets the next, one for each axes | `backend_svg.RendererSVG` |
| `renderer.draw_path`, `draw_markers`, `draw_rectangle`, `draw_polygon`, `draw_text` | Draw lines, markers, rectangles, filled polygons and text, in points from the top left corner; `draw_text` draws a `LogFormatterSciNotation` label with its exponent raised | The renderer methods of the same names, `draw_path_collection` for polygons |
| `renderer.get_text_width_height_descent(text, size)` | About how wide a text is, and the height and depth of its line | `RendererBase.get_text_width_height_descent` |
| `renderer.tostring()` | The SVG file's text | What `savefig` writes |
| `Artist(kind, x, y, widths, color, linewidth, dashes, marker, markersize, label, alpha)` | One line, set of markers, set of bars, set of error bars or filled area, as an axes keeps it; `kind` is `Kind.LINE`, `Kind.MARKERS`, `Kind.BARS`, `Kind.ERRORS`, `Kind.FILL`, `Kind.CONTOUR` or `Kind.FILLED` | `Line2D`, `PathCollection`, `BarContainer`, `LineCollection`, `FillBetweenPolyCollection` |
| `error_bars(x, y, error, across, color, capsize)`, `filled(x, y1, y2, color, alpha)`, `dashes(style, linewidth)` | Build `errorbar`'s and `fill_between`'s artists; a line style's dash lengths | `Axes.errorbar`, `Axes.fill_between`, `Line2D.set_linestyle` |
| `Text(x, y, text, size, ha, va)` | A piece of text as an axes keeps it | `text.Text` |
| `contour_set(x, y, z, levels, filled, colors, cmap, linewidth, alpha, artists)`, `contour_levels(z)`, `contour_lines(x, y, z, level)`, `contour_bands(x, y, z, lower, upper, lowest)` | Build `contour`'s and `contourf`'s artists; matplotlib's levels; one level's lines; one band's polygons | `ContourSet`, `ContourSet._autolev`, `contourpy`'s `mpl2014` |
| `cmap.to_hex(x)` | The color of `x` as `#rrggbb`, each channel rounded to the nearest byte | `colors.to_hex(cmap(x))` |
| `View(left, top, right, bottom, x0, x1, y0, y1, log_x, log_y)` | Where an axes sits, in points, the data range it shows and which axes are logarithmic; `view.to_x(x)` and `view.to_y(y)` map data to points | `Axes.transData` |
| `Figure(axes, nrows, ncols)`, `fig.savefig(fname)` | A grid of axes, as `subplots` makes it, and its file | `figure.Figure` |
| `subplot_box(row, column, nrows, ncols)` | The box, in points, of one cell of a grid in the default figure | `GridSpec.get_grid_positions` |
| `draw_axes(ax, cell, renderer)` | Finds an axes' ranges, places its box in its cell and draws all of it | `Figure.draw`, `Axes.draw` |
| `check_format(fname)` | The format the file's extension names, `"svg"` or `"png"`; raises `savefig`'s `ValueError` for any other | `FigureCanvasBase.savefig`'s check |
| `draw_legend(artists, loc, view, renderer)` | Draws a legend of the labelled artists at `loc`, `"best"` included | `legend.Legend` |
| `location_code(loc)` | matplotlib's number for a legend place: 0 for `"best"`, 1 to 10, or -1 | `Legend.codes` |
| `Image(data, cmap, vmin, vmax, extent, upper)` | An image as an axes keeps it | `image.AxesImage` |
| `draw_image(image, view, renderer)`, `normalize(value, vmin, vmax)` | Draws an image where a view puts it; maps a value to 0 to 1 | `AxesImage.draw`, `colors.Normalize` |
| `draw_colorbar(image, left, top, width, height, label, renderer)` | Draws an image's colorbar in a rectangle | `colorbar.Colorbar` |
| `get_cmap(name)`, `cmap.to_rgba_bytes(x)` | A colormap by name; the color of `x` as four bytes | `colormaps[name]`, `cmap(x, bytes=True)` |
| `renderer.draw_image(x, y, width, height, pixels, smooth, clip)` | Draws an `Array[u8, 3]` of RGBA pixels stretched over a rectangle | `RendererSVG.draw_image` |
| `encode(pixels)` | The bytes of a PNG file of an `Array[u8, 3]` of RGBA pixels, its data uncompressed, as an SVG file embeds it | `_png.write_png` |
| `write_png(pixels)` | The same PNG, its data compressed with `zlib`, as `savefig` writes it | `_png.write_png` |
| `renderer_agg(width, height, dpi)` | A white image `width` by `height` points at `dpi` dots an inch, which takes the same calls as `RendererSVG`; `renderer.print_png()` gives its PNG file | `backend_agg.RendererAgg` |
| `coverage(x, y, left, top, width, height)`, `stroke(x, y, width, dashes, projecting)`, `circle(cx, cy, radius, xs, ys)`, `bounds(x, y)` | How much of each pixel of a box polygons cover, exactly; the pieces that cover a line; a circle as a polygon; the box around points | The Anti-Grain Geometry rasterizer and its `vcgen_stroke` |
| `load()`, `font.width(text)`, `font.outline(text, size, x, y, rotate)` | DejaVu Sans read from `dejavu_sans`; how wide a text is, with kerning; its outline as polygons | `ft2font.FT2Font`, `set_text`, `draw_glyphs_to_bitmap` |
| `approximate_width(text)`, `math_layout(text, size)` | A text's width without reading the font, as the SVG renderer lays text out; where the pieces of a `LogFormatterSciNotation` label go | `mathtext` |
| `nonsingular(low, high, expander, tiny)`, `fixed_limits(first, second)`, `autoscale(artists, images, along_x, log)` | Widen a range too narrow to show; keep limits as `set_xlim` does; the range data needs | `transforms.nonsingular`, `Axes.set_xlim`, `Axes.autoscale_view` |

Coordinates are in points from the figure's top left corner, y growing downwards, as SVG has
them. The PDF backend (roadmap 3.15.7) will take the same calls.

## SVG files

A figure is 460.8 by 345.6 points, 6.4 by 4.8 inches, with the axes in matplotlib's default box.
Text is written as SVG text in DejaVu Sans, matplotlib's font, as matplotlib writes it with
`svg.fonttype = "none"`, so it stays searchable and editable; a viewer without DejaVu Sans draws
it in Arial, Helvetica or another sans-serif font, and labels may then sit slightly off. Every
point of every line is written, about 15 bytes a point, so a line of a million points makes a
file of about 15 MB. An image costs about 5.5 bytes a cell, since its PNG is not compressed
(idea I.54), so a 1000 by 1000 image makes a file of about 5.5 MB.

## PNG files

`savefig` writes a PNG image when the file's name ends in `.png`, in any case, as matplotlib
does:

```python
import byplot as plt


def main() -> Raises[None, OSError | ValueError]:
    t = Array([0.0, 1.0, 2.0, 3.0])
    ax = plt.axes()
    ax.plot(t, t * t, marker="o", label="µ")
    ax.set_title("Growth of µ with ΔT")
    ax.legend()
    ax.savefig("growth.png")
```

The image is 640 by 480 pixels, the 6.4 by 4.8 inch figure at matplotlib's 100 dots an inch,
with white behind it and every pixel fully opaque. It is drawn with anti-aliasing: each edge pixel
takes the color by the share of it the shape covers, worked out exactly. Lines and rectangles
whose sides all run straight across or down, such as the axes' frame, ticks and grid lines, are
moved onto the pixel grid first, as matplotlib's snapping moves them, so they stay sharp.

Text is drawn from the outlines of DejaVu Sans, compiled into Baryon, so a PNG looks the same on
every machine whether or not the font is installed. Its widths and kerning are the font's own,
so text is placed more exactly than in an SVG file, whose viewer draws the text. The font has
the printable ASCII characters, the Greek letters, `×`, `−`, `°`, `±`, `µ` and `·`; any other
character is drawn as the font's empty box. Text is not hinted, so small text looks a little
softer than matplotlib's.

The pixels are compressed with `zlib`, so a typical plot makes a file of about 50 kB. Drawing
costs about one step a pixel of the box around each piece of a line or shape, and a marker is
drawn once and copied onto each point. Points of a line closer than a quarter of a pixel to the
last one kept are passed over, as matplotlib's path simplification passes them, so a line of a
million points takes about a tenth of a second in a `--release` build. The images are
not pixel-identical to matplotlib's, which hints its text and draws in its newer default font;
they show the same plot at the same size and place.

## Errors

`savefig` raises `ValueError` for any extension but `.svg` and `.png`, with matplotlib's message,
such as `Format 'jpg' is not supported (supported formats: png, svg)`, and `OSError` when the file
cannot be written. `plot`, `scatter`, `bar`, `errorbar` or `fill_between` with arrays of different
lengths, an unknown color, line style, legend place, colormap, `errorbar` format or `text`
alignment, an `imshow` `origin` or `aspect` it does not know, an `alpha` outside 0 to 1, contour
levels that do not rise or a grid that does not match its `x` and `y`, a `hist` with fewer than one
bin, a `subplots` grid with no rows or columns, or a `colorbar` before any image panics, and so
does `savefig` on an unknown marker, where matplotlib raises `ValueError` or `RuntimeError`: each
is a mistake in the program, not in its data ([decision 0046](../decisions/0046-plotting.md)).
