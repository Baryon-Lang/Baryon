# Plotting

The `pyplot` module draws plots with matplotlib's names and matplotlib's default look, written in
Baryon and saved as SVG files that any browser, VS Code or a LaTeX toolchain can show
([decision 0046](../decisions/0046-plotting.md)). The `ticker` module beside it places and
labels the ticks, as `matplotlib.ticker` does. These two are the modules a program imports.
`pyplot` draws through smaller files of the standard library, named as matplotlib names its own:
`colors` reads color names, `cm` holds the colormaps, `artist` holds what an axes draws,
`transforms` finds the ranges it shows, `axis` draws an axis's ticks and labels, `legend`,
`image` and `colorbar` draw a legend, an image and a colorbar, `backend_svg` writes the SVG file,
and `png` the PNG images inside it. They are internal, so a program's own `colors.byn` or
`image.byn` is never hidden by them.

This chapter covers what builds today, roadmap 3.15.1 to 3.15.3: line plots, scatter plots,
bars, histograms, markers, legends, grid lines, images and colorbars, titles, axis labels, axis
ranges and SVG output. Grids of plots, logarithmic axes, contours, PNG and PDF files follow in
roadmap 3.15.4 to 3.15.7.

## A first plot

A damped oscillator's position and velocity over ten seconds:

```python
from math import cos, exp, sin
import pyplot as plt


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
| `import matplotlib.pyplot as plt` | `import pyplot as plt` | Libraries are named without their package, as `linalg` is for `numpy.linalg` |
| `fig, ax = plt.subplots()` | `ax = plt.axes()` | Baryon values have one owner, so `fig` and `ax` cannot share a figure: drawing on `ax` would not change `fig` |
| `fig.savefig("p.svg")` | `ax.savefig("p.svg")` | The axes are saved in a figure of their own |
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
| `ax.legend(loc="best")` | Draws a legend of every labelled line, set of markers and set of bars | `Axes.legend` |
| `ax.grid(visible=True)` | Draws light gray lines across the axes at every tick, or stops drawing them | `Axes.grid` |
| `ax.imshow(X, cmap="viridis", vmin=None, vmax=None, origin="upper", aspect="equal", extent=None)` | Draws the `Array[f64, 2]` `X` as an image, one colored cell per element | `Axes.imshow` |
| `ax.colorbar(label="")` | Draws a colorbar of the last image on the right of the axes | `Figure.colorbar(im, ax=ax, label=...)` |
| `ax.set_title(label)` | Sets the title, centred above the axes, at 12 points | `Axes.set_title` |
| `ax.set_xlabel(xlabel)`, `ax.set_ylabel(ylabel)` | Sets the label under the x axis, or left of the y axis | `Axes.set_xlabel`, `Axes.set_ylabel` |
| `ax.set_xlim(left, right)`, `ax.set_ylim(bottom, top)` | Fixes the range shown; a first value larger than the second turns the axis around | `Axes.set_xlim`, `Axes.set_ylim` |
| `ax.savefig(fname)` | Writes a figure holding the axes; the extension picks the format | `Figure.savefig` |

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
import pyplot as plt


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
import pyplot as plt


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

### Markers

`marker` is one of matplotlib's: `"o"` a circle, `"."` a small circle, `"s"` a square, `"^"`
and `"v"` triangles pointing up and down, `"D"` a diamond, and `"+"` and `"x"` crosses. In
`plot` it is `markersize` points across; a `linestyle` of `""` or `"None"` leaves the line out,
so only the markers show. Each marker is written once in the file and placed at every point, so
a point costs about 60 bytes.

### Legends

A legend lists the artists with a `label`, in the order they were drawn, each with a short
sample of its line, marker or bar. `loc` is `"best"`, or one of matplotlib's places:
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
plot's first line and its first bars are both blue. A borrowed `str | None` parameter does not build yet, so `""` stands for "none" here.

`linestyle` is `"-"` or `"solid"`, `"--"` or `"dashed"`, `":"` or `"dotted"`, and `"-."` or
`"dashdot"`. Dashes are matplotlib's, measured in line widths, so a thicker line has longer
dashes.

`pyplot` turns each name into the `"#rrggbb"` form an SVG file holds, as matplotlib's
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

## The files under `pyplot`

`pyplot` draws through these internal files of the standard library. A program cannot import
them; this table is for those working on the library, and on the PNG and PDF backends to come.

| Item | Does | matplotlib |
| --- | --- | --- |
| `XAxis(vmin, vmax, left, right, bottom, log)` | The axis along the bottom of an axes, laid from `left` to `right` points at `bottom`, by the logarithm of the value when `log` is set | `axis.XAxis` |
| `YAxis(vmin, vmax, bottom, top, left, log)` | The axis along the left of an axes | `axis.YAxis` |
| `axis.get_majorticklocs()`, `axis.get_minorticklocs()` | The tick positions, with as many intervals as fit the axis's length; minor ticks only on a logarithmic axis | `Axis.get_majorticklocs`, `Axis.get_minorticklocs` |
| `axis.draw(renderer)` | Draws the ticks, minor ticks shorter and thinner, their labels and the offset text; returns where the axis label goes | `Axis.draw` |
| `RendererSVG(width, height, [])` | An empty SVG drawing, sizes in points | `backend_svg.RendererSVG` |
| `renderer.draw_path`, `draw_markers`, `draw_rectangle`, `draw_text` | Draw lines, markers, rectangles and text, in points from the top left corner; `draw_text` draws a `LogFormatterSciNotation` label with its exponent raised | The renderer methods of the same names |
| `renderer.get_text_width_height_descent(text, size)` | About how wide a text is, and the height and depth of its line | `RendererBase.get_text_width_height_descent` |
| `renderer.tostring()` | The SVG file's text | What `savefig` writes |
| `Artist(kind, x, y, widths, color, linewidth, dashes, marker, markersize, label)` | One line, set of markers or set of bars, as an axes keeps it; `kind` is `Kind.LINE`, `Kind.MARKERS` or `Kind.BARS` | `Line2D`, `PathCollection`, `BarContainer` |
| `View(left, top, right, bottom, x0, x1, y0, y1)` | Where an axes sits, in points, and the data range it shows; `view.to_x(x)` and `view.to_y(y)` map data to points | `Axes.transData` |
| `draw_legend(artists, loc, view, renderer)` | Draws a legend of the labelled artists at `loc`, `"best"` included | `legend.Legend` |
| `location_code(loc)` | matplotlib's number for a legend place: 0 for `"best"`, 1 to 10, or -1 | `Legend.codes` |
| `Image(data, cmap, vmin, vmax, extent, upper)` | An image as an axes keeps it | `image.AxesImage` |
| `draw_image(image, view, renderer)`, `normalize(value, vmin, vmax)` | Draws an image where a view puts it; maps a value to 0 to 1 | `AxesImage.draw`, `colors.Normalize` |
| `draw_colorbar(image, left, top, width, height, label, renderer)` | Draws an image's colorbar in a rectangle | `colorbar.Colorbar` |
| `get_cmap(name)`, `cmap.to_rgba_bytes(x)` | A colormap by name; the color of `x` as four bytes | `colormaps[name]`, `cmap(x, bytes=True)` |
| `renderer.draw_image(x, y, width, height, pixels, smooth, clip)` | Draws an `Array[u8, 3]` of RGBA pixels stretched over a rectangle | `RendererSVG.draw_image` |
| `encode(pixels)` | The bytes of a PNG file of an `Array[u8, 3]` of RGBA pixels, data uncompressed until roadmap 3.27 | What `Figure.savefig` writes to a `.png` |
| `nonsingular(low, high, expander, tiny)`, `fixed_limits(first, second)`, `autoscale(artists, images, along_x)` | Widen a range too narrow to show; keep limits as `set_xlim` does; the range data needs | `transforms.nonsingular`, `Axes.set_xlim`, `Axes.autoscale_view` |

Coordinates are in points from the figure's top left corner, y growing downwards, as SVG has
them. The PNG and PDF backends (roadmap 3.15.6 and 3.15.7) will take the same calls.

## SVG files

A figure is 460.8 by 345.6 points, 6.4 by 4.8 inches, with the axes in matplotlib's default box.
Text is written as SVG text in DejaVu Sans, matplotlib's font, as matplotlib writes it with
`svg.fonttype = "none"`, so it stays searchable and editable; a viewer without DejaVu Sans draws
it in Arial, Helvetica or another sans-serif font, and labels may then sit slightly off. Every
point of every line is written, about 15 bytes a point, so a line of a million points makes a
file of about 15 MB. An image costs about 5.5 bytes a cell, since its PNG is not compressed
until roadmap 3.27, so a 1000 by 1000 image makes a file of about 5.5 MB.

`savefig` raises `ValueError` for any extension but `.svg`, with matplotlib's message, such as
`Format 'png' is not supported (supported formats: svg)`, and `OSError` when the file cannot be
written. `plot`, `scatter` or `bar` with arrays of different lengths, an unknown color, line
style, legend place or colormap, an `imshow` `origin` or `aspect` it does not know, a `hist`
with fewer than one bin, or a `colorbar` before any image panics, and so does `savefig` on an
unknown marker, where matplotlib raises `ValueError` or `RuntimeError`: each is a mistake in the
program, not in its data
([decision 0046](../decisions/0046-plotting.md)).
