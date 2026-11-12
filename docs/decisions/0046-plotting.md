# 0046. Plots are drawn with matplotlib's object-oriented names, in a `pyplot` module, to SVG first

Status: Accepted (Collins, 2026-11-12)
Date: 2026-11-12
Roadmap: 3.15

## Context

Roadmap 3.15 gives Baryon plots: the line, scatter, bar, histogram and image plots a physicist or
data scientist makes every day, written to files that go into a paper, a report or a notebook.
Phase 3's gate asks for a 30-line script that loads, groups, fits and plots.

Native first is the rule (Collins, 2026-11-10): the plotting library is Baryon code in the
standard library (decision 0038), never a call into Python or matplotlib. Its names follow the
library readers already know, as NumPy's (0040, 0042, 0043) and AstroPy's (0039, 0044) do. For
plots that library is matplotlib, and the code its users write looks like this:

```python
import matplotlib.pyplot as plt

fig, ax = plt.subplots()
ax.plot(t, x, label="position")
ax.set_xlabel("time (s)")
ax.legend()
fig.savefig("oscillator.svg")
```

Two parts of matplotlib cannot carry over as they are:

- **The implicit current figure.** `plt.plot(t, x)` then `plt.savefig("a.svg")` works because
  `pyplot` keeps the current figure in a module-level variable. Baryon has no module-level
  variables: they are shared mutable state, which a parallel loop would race on
  ([source files](../reference/syntax.md#source-files)).
- **`fig, ax = plt.subplots()`.** In matplotlib `fig` and `ax` are two references to objects that
  point at each other, so drawing on `ax` changes what `fig.savefig` writes. Baryon values are
  owned, not shared ([values and references](../reference/values-and-references.md)): unpacking
  would give two separate values, and `fig.savefig` would write an empty figure. First-class
  references are roadmap 3.6, months away.

Three ways were weighed:

- **matplotlib's object-oriented names, with the figure owning its axes.** Every method that draws
  is an `Axes` method, as matplotlib recommends for all but the shortest scripts. One plot is
  `ax = plt.axes()`, drawn on, then `ax.savefig(path)`. A grid of plots is
  `fig = plt.subplots(2, 1)`, drawn on as `fig.axes[0].plot(...)`, and saved with
  `fig.savefig(path)`. Works today and gives up only the two spellings above.
- **A hidden current figure kept by the runtime**, so `plt.plot` and `plt.savefig` work. Breaks
  the rule against global mutable state, races inside `prange`, and is the style matplotlib's own
  documentation steers people away from.
- **Waiting for references (3.6)** so `fig, ax = plt.subplots()` works exactly. Delays every plot
  by months for one line of difference.

## Decision

1. **A `pyplot` module with matplotlib's names**, imported as matplotlib users do,
   `import pyplot as plt`, without the `matplotlib.` prefix, as `linalg` drops `numpy.`. It is
   written in Baryon in the standard library.
2. **Drawing is done on an `Axes`**, with matplotlib's method names and keyword arguments:
   `plot`, `scatter`, `bar`, `hist` and `imshow`, then `errorbar`, `fill_between`, `text`,
   `contour` and `contourf`; `set_title`, `set_xlabel`, `set_ylabel`, `set_xlim`, `set_ylim`,
   `set_xscale` and `set_yscale`, `legend`, `grid`, and `colorbar` for an image.
3. **Figures own their axes.** `plt.axes()` gives a new `Axes`; `ax.savefig(fname)` writes a
   figure holding only it. `plt.subplots(nrows, ncols)` gives a `Figure` whose `fig.axes` is the
   list of its axes, row by row as matplotlib's is, and `fig.savefig(fname)` writes it.
   `Axes.savefig` is Baryon's addition; matplotlib has `fig.savefig` only.
4. **No implicit current figure.** `plt.plot`, `plt.xlabel`, `plt.savefig` and the rest of
   pyplot's state-machine functions are not provided, and a program that calls them is told the
   name does not exist. When roadmap 3.6 brings references, `fig, ax = plt.subplots()` can be
   added under a new record without breaking what is written by then.
5. **matplotlib's default look.** The figure is 6.4 by 4.8 inches, the axes fill matplotlib's
   default box, colors come from its `tab10` cycle, text is DejaVu Sans at 10 points, images use
   `viridis`, and line widths, tick lengths, margins and legend spacing are its defaults. Tick
   positions are the ones matplotlib's `AutoLocator` picks, and tick labels are written as its
   `ScalarFormatter` writes them, minus sign U+2212 included. Tests check the tick positions
   against numbers recorded from matplotlib, and each plot type against a stored reference file
   of Baryon's own output. Pixel-identical output to matplotlib is a non-goal. Tick placement
   and labels live in a second module, `ticker`, as matplotlib's `matplotlib.ticker`, with its
   `MaxNLocator.tick_values` and `ScalarFormatter.format_ticks` and `get_offset`, so they can be
   used and tested on their own; `colors`, with the colormaps, follows with images (3.15.3).
6. **Formats, in order: SVG, then PNG, then PDF**, chosen by the file name's extension as
   `savefig` chooses them; any other extension raises `ValueError`. SVG is text, so it needs
   nothing outside Baryon; text is written as SVG text, as matplotlib's `svg.fonttype = "none"`
   writes it, so the viewer draws it. PNG needs a rasterizer with anti-aliasing and a font's
   outlines, both written in Baryon, with DejaVu Sans (under the Bitstream Vera license, which
   permits embedding) compiled in; until compression (3.27) lands its image data is stored in
   uncompressed deflate blocks, a valid PNG about four times larger. PDF follows PNG.
7. **Data are `Array[f64, 1]` first** (images `Array[f64, 2]`), as `stats` takes. Lists, integer
   arrays and other element types come with overloading or generic functions over the element
   type.
8. **Wrong arguments panic**, as in decision 0042: `x` and `y` of different lengths, fewer than
   one bin, an unknown line style or color name. They are mistakes in the program, where
   matplotlib raises `ValueError`. A file that cannot be written raises `OSError`.
9. **Out of scope:** interactive windows and `plt.show()`, 3D axes, animation, polar axes,
   mathematical text with `$...$`, styles and `rcParams`, and dates on an axis. Each can come
   back as an idea in the roadmap.

## Consequences

- A matplotlib user reads a Baryon plot script at a glance and writes one by changing
  `fig, ax = plt.subplots()` to `ax = plt.axes()` and `fig.savefig` to `ax.savefig`. A script that
  uses the state-machine style fails to compile with the missing name, instead of drawing
  something else.
- Plots have no dependency outside the compiler, so they work the same on every platform and in
  CI, and reference files can be compared byte for byte.
- SVG files show in any browser, in VS Code, and in LaTeX after conversion; journals that want a
  bitmap wait for PNG (3.15.6).
- Text drawn by the viewer can differ slightly in width from DejaVu Sans when that font is not
  installed; layout uses DejaVu Sans's widths, so labels may sit a little off on such a machine.
- AstroBaryon's visualization (4.2.10) builds on this module: its image stretches and sky axes
  are added to the same `Axes`.
- Plotting large data is linear in the number of points, and an SVG holds every point, so a
  line of ten million points writes a file of about 150 MB. Thinning lines to the pixels they
  cover, as matplotlib's path simplification does, is a later optimization.
