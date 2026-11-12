# Plotting

The `pyplot` module draws plots with matplotlib's names and matplotlib's default look, written in
Baryon and saved as SVG files that any browser, VS Code or a LaTeX toolchain can show
([decision 0046](../decisions/0046-plotting.md)). The `ticker` module beside it places and
labels the ticks, as `matplotlib.ticker` does.

This chapter covers what builds today, roadmap 3.15.1: line plots, titles, axis labels, axis
ranges and SVG output. Scatter plots, bars, histograms, legends, grids, images, grids of plots,
logarithmic axes, contours, PNG and PDF follow in roadmap 3.15.2 to 3.15.7.

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
| `plt.plot(t, x)`, `plt.savefig(...)` | not provided | These draw on a hidden current figure, a module-level variable, and Baryon has none: a parallel loop would race on it |

Everything else keeps matplotlib's names: the methods, their keyword arguments and the color and
line style names. A script that uses `plt.plot` fails to compile with the missing name, so a
moved script is never drawn wrong in silence.

## `Axes`

| Method | Does | matplotlib |
| --- | --- | --- |
| `ax.plot(x, y, color="", linewidth=1.5, linestyle="-", label="")` | Draws the points `(x[i], y[i])` joined by straight lines | `Axes.plot(x, y, ...)` |
| `ax.set_title(label)` | Sets the title, centred above the axes, at 12 points | `Axes.set_title` |
| `ax.set_xlabel(xlabel)`, `ax.set_ylabel(ylabel)` | Sets the label under the x axis, or left of the y axis | `Axes.set_xlabel`, `Axes.set_ylabel` |
| `ax.set_xlim(left, right)`, `ax.set_ylim(bottom, top)` | Fixes the range shown; a first value larger than the second turns the axis around | `Axes.set_xlim`, `Axes.set_ylim` |
| `ax.savefig(fname)` | Writes a figure holding the axes; the extension picks the format | `Figure.savefig` |

`x` and `y` are `Array[f64, 1]`s of the same length; `plot` copies them, so they can change
afterwards. Lists and integer arrays come with generic functions over the element type: until
then, `Array([1.0, 2.0])` makes an array of a list.

A point with a NaN or infinite coordinate is left out and breaks the line there, as matplotlib
breaks it, so missing measurements show as gaps.

### Colors and line styles

`color` takes the names matplotlib takes:

| Name | Color |
| --- | --- |
| `"#rrggbb"` | That color, in hexadecimal |
| `"C0"` to `"C9"` | The colors of the cycle, in order |
| `"tab:blue"`, `"tab:orange"`, `"tab:green"`, `"tab:red"`, `"tab:purple"`, `"tab:brown"`, `"tab:pink"`, `"tab:gray"`, `"tab:olive"`, `"tab:cyan"` | The same colors by name |
| `"b"`, `"g"`, `"r"`, `"c"`, `"m"`, `"y"`, `"k"`, `"w"` | Blue, green, red, cyan, magenta, yellow, black and white, as matplotlib defines the letters |

Left out, or `""`, each line takes the next color of the cycle, as matplotlib's `color=None`
does. A borrowed `str | None` parameter does not build yet, so `""` stands for "none" here.

`linestyle` is `"-"` or `"solid"`, `"--"` or `"dashed"`, `":"` or `"dotted"`, and `"-."` or
`"dashdot"`. Dashes are matplotlib's, measured in line widths, so a thicker line has longer
dashes.

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

## SVG files

A figure is 460.8 by 345.6 points, 6.4 by 4.8 inches, with the axes in matplotlib's default box.
Text is written as SVG text in DejaVu Sans, matplotlib's font, as matplotlib writes it with
`svg.fonttype = "none"`, so it stays searchable and editable; a viewer without DejaVu Sans draws
it in Arial, Helvetica or another sans-serif font, and labels may then sit slightly off. Every
point of every line is written, about 15 bytes a point, so a line of a million points makes a
file of about 15 MB.

`savefig` raises `ValueError` for any extension but `.svg`, with matplotlib's message, such as
`Format 'png' is not supported (supported formats: svg)`, and `OSError` when the file cannot be
written. `plot` with `x` and `y` of different lengths, an unknown color or an unknown line style
panics, where matplotlib raises `ValueError`: each is a mistake in the program, not in its data
([decision 0046](../decisions/0046-plotting.md)).
