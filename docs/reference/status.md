# What builds today

The rest of this reference describes the whole of version 0. The compiler does not build all of it
yet. This chapter says, chapter by chapter, what `baryon build`, `baryon run` and `baryon test`
compile into a working program today, and what they still reject.

Two stages decide that. `baryon check` already accepts every rule in the reference: a program
that breaks one gets the error the reference describes. Building goes further: lowering turns the
checked program into the mid-level IR (roadmap 1.9.3), and the LLVM backend turns that into machine
code (roadmap 1.10). Each grows one feature at a time. A feature either stage cannot handle yet
is reported as an error that names it and the stage, never miscompiled:

```text
error: lowering does not handle `in` yet
 --> main.byn:3:12
  |
3 |     print(n in [1, 2])
  |            ^^^^^^^^^^ not lowered yet
  = note: the compiler lowers more of the language as roadmap item 1.9.3 continues
```

So a program that `baryon check` accepts but `baryon build` rejects is not wrong: it uses a part of
the language that is specified and checked, but not built yet. The editor sees the same split:
[Coding Baryon in VS Code](../guide/vscode.md) underlines every error `baryon check` finds as you
type, while `baryon run` builds only what this page lists.

This page was last brought up to date on 2026-11-13, when PDF files from `savefig`, and before them
NetCDF-4 files read with `netCDF4.Dataset`, and before them PNG files from `savefig`, and before
them HDF5 files read with `h5by.File`, its groups, datasets and attributes, and before them
logarithmic axes, and before them images and colorbars, and before them text columns of a
`DataFrame` joined it, and before them arrays of text, `Array[str, 1]`, and before them
`a.argsort()`, and before it `concat` of `DataFrame`s, and before it `drop_duplicates`, and before
it the inner `merge` of two `DataFrame`s, and before it `groupby(...).agg(...)`, and before it
`lzma.decompress`, and before it `sort_values` on a `DataFrame`, and before it `bz2.compress` and
`bz2.decompress`, and before them `df[["a", "b"]]`, `assign`, `drop` and `rename`, and before them
`for row in df.itertuples()`, and before it the rows of a `DataFrame` chosen by `df[mask]`,
`df[i:j]`, `head` and `tail`, and before them `compression.zstd.compress`, and before it `fft2`,
`ifft2`, `rfft2` and `irfft2`, and before them `compression.zstd.decompress`, and before it
`print(df)`, a frame printed as pandas prints it, and before it `zlib.decompressobj` and `gzip`, and
before them a `DataFrame` built from rows and `df[i]`, and before them `DataFrame[S]` built from
arrays, with its columns, and before them `zlib.compress`, and before it `zlib.decompress`,
`zlib.crc32` and `zlib.adler32`, and before them scatter plots, bars, histograms, legends and grid
lines, and before them comparisons of arrays and `a[mask]`, and before them line plots with `byplot`
and `ticker`, saved as SVG, and before them slices of a NetCDF variable, `var[0, :, ::2]` read and
written, and before them writing NetCDF classic files, `Dataset(path, "w", format=...)`,
`createDimension`, `createVariable`, `setncattr` and `var[:] = values`, and before them reading
NetCDF classic files, `netCDF4.Dataset`, its variables, dimensions and attributes, and
`AttributeError`, and before them FITS images and headers, `fits.getdata`, `fits.getheader`,
`fits.writeto` and `KeyError`, and before them, on 2026-11-11, the standard streams, `print(...,
file=)` and `input`, `.npy` files, `Array.load` and `Array.save`, `linalg.svd`, `svdvals`, `eig` and
`eigvals`, `fft`, text tables, `Array.loadtxt` and `Array.savetxt`, and before them `linalg.eigh`,
`eigvalsh`, `cholesky` and `qr`, `solve`, `lu_factor`, `lu_solve`, `det` and `inv` and, before them,
array files, `Array.fromfile` and `a.tofile`, and before them `for line in f` over a text file,
`int(text)` and `float(text)`, began to build. Before that, on 2026-11-10, text files, `open`, the
`File` methods and `with open(...) as f`, then `OSError`, `os.ErrorKind`, unions of structs and,
before them, `sys.argv`, `sys.exit` and `os.getenv` began to build. Every pull request that teaches
lowering or the backend something new updates the table it affects, in the same pull request, as
roadmap item 0.8.1 asks.

## The example programs

All six [example programs](../examples/README.md) build and run:
[`heat_equation.byn`](../examples/heat_equation.byn),
[`lennard_jones.byn`](../examples/lennard_jones.byn),
[`lorenz_rk4.byn`](../examples/lorenz_rk4.byn),
[`conjugate_gradient.byn`](../examples/conjugate_gradient.byn),
[`monte_carlo_pi.byn`](../examples/monte_carlo_pi.byn) and
[`rod_boundaries.byn`](../examples/rod_boundaries.byn). Each is built and run by a test that
checks what it prints.

| Example | Still needs |
| --- | --- |
| [`heat_equation.byn`](../examples/heat_equation.byn) | Nothing: it builds and runs |
| [`lennard_jones.byn`](../examples/lennard_jones.byn) | Nothing: it builds and runs |
| [`lorenz_rk4.byn`](../examples/lorenz_rk4.byn) | Nothing: it builds and runs |
| [`conjugate_gradient.byn`](../examples/conjugate_gradient.byn) | Nothing: it builds and runs |
| [`monte_carlo_pi.byn`](../examples/monte_carlo_pi.byn) | Nothing: it builds and runs |
| [`rod_boundaries.byn`](../examples/rod_boundaries.byn) | Nothing: it builds and runs |

## Syntax

| Builds today | Not built yet |
| --- | --- |
| Packages of several modules; `import m`, `from m import x`, `as`; calling a function, reading a constant and naming an enum member through its module, `physics.units.twice(x)`, `units.C`, `units.Phase.GAS` | |
| `main` in `main.byn`; [`print`](syntax.md#what-print-writes-for-each-value) of numbers, `bool`, `str`, `None`, `T \| None`, tuples, enum members, structs, lists and dicts, with `sep=`, `end=`, [`file=` and `flush=`](files-and-processes.md#standard-streams); arrays of numbers and `bool`, [as NumPy writes them](arrays.md#printing) | `print` of arrays of `str` or structs; a tuple holding `None` |
| Assignment, annotated assignment, augmented assignment including `**=`; unpacking a tuple or a rank-1 `Fixed` array, `a, b = b, a`, and nested unpacking, `a, (b, c) = e` | |
| `if`, `elif`, `else`, `while`, `break`, `continue`, `pass`, `return`, `assert` | |
| `for` over `range`, with a step that is a constant or computed at run time, where a zero step panics; over a rank-1 `Array` or `Fixed` array; over a list, such as `list[f64]` or `list[str]`; over a tuple whose items have one type, such as `(2, 3, 5)`; over the characters of a `str`, each a one-character `str`; over `enumerate`, `zip` and `reversed` of these, nested in each other; a loop over values that are not copied, such as `str`s, borrows each element; with a target that unpacks, `for x, y in points` | A `prange` step that is not a constant; `reversed` of a `str`; a `range` step that is not a constant inside `enumerate`, `zip` or `reversed`; `enumerate`, `zip` and `reversed` outside a `for` loop or comprehension, such as `list(zip(a, b))` |
| `def` at module level and methods; keyword arguments; [default parameter values](types.md#default-parameter-values), each evaluated at every call that leaves its argument out | Leaving out an argument of a generic function |
| `class` with `Struct`, `Copy`, `Exception`, `Enum` and `Protocol` bases, and the traits a struct names; `@layout("C")` | |
| `raise` and `try`/`except`, including bare `raise` | |
| `match` on numbers, `bool`, enums, `T \| None` and tuples of these, with `\|` patterns and guards | `match` on `str`; `\|` patterns that capture names |
| `with unsafe:` | |
| Number, string, `True`, `False` and `None` literals, with Python's backslash escapes such as `\n`, `\t` and `\u00e9`, and raw strings `r"..."`; f-strings with format specifications; complex literals such as `2j` | `\N{name}` escapes; bytes `b"..."`; a literal written in parts, `"a" "b"` |
| `+ - * / // % **`, unary `-` and `+`, bitwise operators, chained comparisons, `and`, `or`, `not`, `is None`, conditional expressions; comparisons, `in` and `not in` between two `str`s | `in` and `not in` on arrays and tuples |
| `min`, `max`, `len` of arrays, lists and `str`; `sum`, `min` and `max` of a list or rank-1 array of numbers, a list's float `sum` as CPython 3.12 adds it; `int`, `float` and the number conversions such as `f64(n)`; the [`math`](arrays.md#math-functions) functions for real numbers, from `sqrt` and `atan2` to `erf`, `fma`, `floor` and `isnan`, and the constants `pi`, `e`, `tau`, `inf` and `nan`; [`math.fsum`](arrays.md#exact-sums) of a list or rank-1 array of `f64`s; [`math.f64x2`](arrays.md#double-double-floats) double-double floats and their methods; [`random.Philox`](arrays.md#random-numbers) and `uniform()`; `round(x)` and `round(x, digits)` as Python rounds; `sorted` of a list of numbers or `str`s, with `reverse=`; `abs` of integers, floats and complex numbers | `float(text)` and the other conversions of a `str` to a number; `sorted` with `key=` or of other elements, such as tuples; `min` and `max` of a list of `str`s; `sum` of a list of complex numbers and of an array with a start; the integer functions of `math`, such as `gcd`, and `math.prod`; `fsum` of other element types and iterables; the rest of `random` |
| `@test` functions, with `baryon test` | |

## Types

| Builds today | Not built yet |
| --- | --- |
| Every integer type, `f32`, `f64` and `bool`, with Python's number mixing, `//` and `%` | |
| [Complex numbers](types.md#complex-numbers) `complex64` and `complex128`: arithmetic, `==`, `real`, `imag`, `conjugate()`, `abs`, `complex(re, im)`, `print`, and f-strings with format specifications | `a.sum()` of a complex array; complex numbers in `prange` reductions and `accumulate=`; the `cmath` module (roadmap idea I.37) |
| Conversions that panic when the value does not fit | |
| `str`: literals, f-strings, `+`, `+=` on a variable, field or list element, `s.copy()`, `==`, `!=`, `<`, `<=`, `>`, `>=`, `in` and `len`; [indexing and slicing](types.md#indexing-slicing-and-methods) by character; `split`, `strip`, `lstrip`, `rstrip`, `join`, `startswith`, `endswith`, `find`, `replace`, `lower` and `upper`; `int(text)` and `float(text)`, which raise `ValueError` | `str`'s other methods, such as `count`, `rfind` and `isdigit`; `split` with `maxsplit`, `find` with `start` and `end`, and `startswith` of a tuple |
| Tuples: building one, reading `t[0]` with a constant index, and unpacking | |
| `T \| None`, with every narrowing rule | |
| Structs, `Copy` structs, fields, methods, `InOut[Self]` methods, methods called through their class, `Point.origin()`, generic structs and their methods, each method compiled once per type its struct is used at | |
| Enums: members, `==` and `!=`, `match` | |
| [Unions](types.md#unions-of-structs) of structs, enums, error types and `None`: a member passed, assigned or returned where its union is expected, `match` with class patterns by position and keyword, `print`, members that own memory, freed by the member they hold, shared methods called on a union, and unions as the type of a trait-bounded parameter | A shared method that a member gets from an embedded field; `Array.zeros` of a union; class patterns on generic structs; a capture of a field that is not a copy type |
| Type aliases, including as a constructor: `Vec3(1.0, 0.0, 0.0)` | |
| `Final` constants of numbers, including `Final[f32]` and arithmetic on other constants | `Final` constants that are not numbers |
| Generic functions, compiled once per type they are called at, including `VectorSpace` arithmetic on `Fixed` vectors, `math` functions, `/`, in-place operators and `Array.zeros(n, T)` under a `Number` or `Float` bound, and bounds by a [user trait](types.md#using-a-trait-as-a-bound), `[T: Body]`, whose method calls go straight to each type's own method | |
| [Embedded structs](types.md#embedded-structs): `Embed[T]` fields, and their fields and methods reached through the outer struct | Calling a method that an embedded generic struct provides |
| [Functions as arguments](types.md#functions-as-arguments): a module-level function passed to a `Callable` parameter, directly or through a local, compiled once per function passed (decision 0026); a `Callable` parameter passed on to another call or copied into a local | A local that holds one of two different functions depending on a branch, which needs a function value at run time: the build reports it |
| Lists of any type, such as `list[f64]`, `list[str]` or lists of structs, arrays and lists: displays `[a, b]` and `[]`, `xs.append(value)`, `len(xs)`, and `xs[i]` with bounds checks, reading and writing; dropping a list drops its elements; `xs.pop()`, which panics on an empty list; `value in xs` and `not in` on lists of numbers, `bool`s, enums and `str`s; `xs.copy()` and slices `xs[i:j:k]`, each a new list, and assigning into a slice `xs[i:j] = ys`, of lists of copy types and `str`s; list comprehensions `[f(x) for x in xs if p(x)]`, with any number of `for` clauses and conditions, over what `for` loops over, and over `simd(range(…))`, which runs as its plain range | `in`, `copy()` and slices on lists of other types; assigning into a list slice with a step, `xs[::2] = ys` |
| [Dictionaries](types.md#dictionaries) with integer, `bool`, `str`, enum and tuple keys and values of any type: displays, `d[k]` read, written and updated in place, with the missing-key panic; `in`, `len`, `get` and `setdefault` of copy types and `str`, `pop` with and without a default, `update`, `clear` and `del d[k]`; dict comprehensions; `for` loops and list comprehensions over the dict, `keys()`, `values()` and `items()`, in insertion order, borrowing keys and values that do not copy; `copy()` and `update` where keys and values are copy types or `str`; dropping a dict drops its keys and values | `copy()` and `update` of keys and values that are not copy types or `str`, such as tuple keys holding a `str` or `list` values |

Integer overflow ([decision 0030](../decisions/0030-integer-overflow.md)): a debug build panics
on `+`, `-`, `*`, `**`, `+=`, `-=`, `*=`, `**=` and `-x` that overflow, and `--release` wraps. That includes
arithmetic in a generic function whose type parameter is an integer, `abs` of the type's minimum,
whole-array arithmetic on integer arrays, integer `a.sum()` and `a.dot(b)`, and `@` on integer
arrays (roadmap 1.11.8).
Indices the compiler computes itself, such as a `range` loop's counter, are not checked.

## Values and references

Every rule in this chapter is checked by `baryon check`, so a program that breaks one never builds.
What it means at run time is built: values move and copy as described, read-only borrows, `InOut`
and `Owned` parameters work, and every value is destroyed when its owner goes away, on every path
out of a function, including through a `raise`.

| Builds today | Not built yet |
| --- | --- |
| Moves, copies, `.copy()` on `str`, `Array` and lists of copy types or `str`s | `.copy()` on structs and on lists of other types |
| `InOut` and `Owned` parameters, including `InOut` arrays and whole-array arithmetic on them | |
| [Slices](values-and-references.md#slices) passed to a read-only parameter, which borrows them in place, or to an `InOut` one, which works on a copy written back after the call; assigned into with `=` and `+=` | Binding a slice to a name, which the reference leaves to Phase 3 |

## Arrays

| Builds today | Not built yet |
| --- | --- |
| `Array.zeros`, `Array.ones` and `Array.fill`, with the element type from context, a `dtype` or `f64`; `a.copy()`; `Array(values)` of a list | `Array.zeros` and `Array.ones` of an enum, whose members are not numbers; `Array.fill` with a member works |
| Indexing `a[i, j]` with bounds checks, reading and writing; negative literal indices | |
| [Slices](arrays.md#slicing) `a[i:j]`, `a[i:j:k]`, `a[::-1]` and `grid[:, k]` of `Array` and `Fixed` arrays, with defaults, negative literals, steps known only at run time, and the out-of-range and zero-step panics; read, passed to functions, in whole-array arithmetic, and assigned into in place | |
| `len(a)`, `a.shape`, `a.shape[k]` and `a.size`, of `Array` and `Fixed` arrays, and `print(a.shape)` | |
| `a.unchecked[i]` inside `with unsafe:` | |
| `Fixed` arrays: calling the type with elements, `Vec3.zeros()`, indexing, `len`, unpacking `x, y, z = v`, the methods below, and whole-array arithmetic | |
| Arrays of `Fixed` arrays and of `Copy` structs | |
| Whole-array arithmetic, `+= -= *= /=` and expressions such as `a + 2.0 * b`, with [broadcasting](arrays.md#broadcasting) between arrays of different ranks and of length 1, as NumPy's, and new axes with `None`, `x[:, None]`; [comparisons](arrays.md#comparisons) of arrays, `a > b` and `a == 2.0`, which give arrays of `bool`s, and `&`, `\|`, `^` and `~` on arrays of `bool`s and integers; [choosing elements](arrays.md#choosing-elements) of a rank-1 `Array` with a mask or with positions, `a[mask]` and `a[positions]`, and setting them to one value, `a[mask] = 0.0`; [`a.argsort()`](arrays.md#sorting-positions) of a rank-1 `Array`, stable, with NaN last | A mask or positions on an array of higher rank; an array assigned to a choice, `a[mask] = values`, and `a[mask] += 1.0`; `argsort` of an array of higher rank, with `axis=` and `kind=` |
| [`@`, the matrix product](arrays.md#the-matrix-product), of `Array`s and of `Fixed` arrays: matrix by matrix, matrix by vector and vector by matrix, as a loop nest | `@=`; calling BLAS (roadmap 3.10.2) |
| [Methods](arrays.md#methods): `a.copy()` of an `Array`; of `Array` and `Fixed` arrays, `a.sum()` and `a.dot(b)`, floats in [NumPy's pairwise order](arrays.md#the-order-sum-adds-in), the float sum of a whole-array expression, `(a * b).sum()`, without storing it, `a.min()`, `a.max()`, [`sum`, `min` and `max` along an axis](arrays.md#reductions-along-an-axis) with NumPy's bits, [`a.mean()`, `a.var(ddof)` and `a.std(ddof)`](arrays.md#means-and-variances), of the whole array and along an axis, with NumPy's bits, `a.round()` and `a.fill(value)`; [`print(a)`](arrays.md#printing), as NumPy writes it | Printing an array of `str` or structs; `var` and `std` of integers or complex numbers, and `mean` of integers along an axis |
| [Arrays of text](text-arrays.md), `Array[str, 1]`: built from a list with `Array(words)`, `len`, an element read by position and borrowed, `for` loops with `enumerate`, `reversed` and `zip`, comprehensions, `copy()`, a slice, a mask and positions, each a new array, comparisons with a `str` or another text array, `in`, `print` and f-strings in NumPy's form, as a variable, parameter, return value, item of a list or tuple, dict value and `T \| None` | Assigning an element, `+`, every other method and built-in, `str(names)`, and a struct field of one (roadmap 3.13.3) |
| [The `stats` module](statistics.md): `median`, `quantile`, `percentile`, `average` and `histogram` of an `Array[f64, 1]`, with NumPy's bits; `cov` and `corrcoef` of two, to NumPy's last bit but one | `axis=`, several quantiles at once, the other quantile methods and other element types; `histogram` with other bins or `range=`, `weights=` and `density=`; `cov` and `corrcoef` of more than two variables |

## DataFrames

| Builds today | Not built yet |
| --- | --- |
| [`DataFrame[S]`](dataframes.md) with number and `bool` columns, as a variable, parameter, return value and item of a list, tuple or `T \| None`; frames with the same columns passed for one another | Missing values (3.13.6); a frame as a struct field |
| [Text columns](text-arrays.md#in-a-dataframe), a field `symbol: str`: a frame built from its columns with `DataFrame({"symbol": Array(words), ...})`, the column read as an `Array[str, 1]` and replaced whole, `len(df)`, `df.shape`, `df.columns`, `df.copy()`, `print(df)` as pandas prints it, and rows chosen with `head`, `tail`, a slice, a mask and `sort_values` by any column, text included, the rows that repeat left out with `drop_duplicates`, the inner `merge` on a key of text or numbers, `groupby` by text keys with text columns reduced by `count`, `first`, `last`, `min` and `max`, `DataFrame(rows)` from structs with `str` fields, and one row as a struct that owns its text, `df[i]` and `itertuples`, and columns chosen and changed with `df[[...]]`, `assign`, `drop` and `rename`, and `concat` | pandas's `sum` of a text column in `agg`, which joins the texts |
| [Building a frame](dataframes.md#building-a-frame) from arrays, `DataFrame({"price": prices})`, with the panic for columns of two lengths, and from a list of rows, `DataFrame(rows)` | |
| [Columns](dataframes.md#columns) read as `df.price` and `df["price"]`, used as arrays, changed in place and replaced, with the panic for a new column of another length; `len(df)`, `df.shape`, `df.columns`, `df.copy()`; [a row](dataframes.md#rows), `df[i]`, as the schema's struct; [`print(df)` and a frame in an f-string](dataframes.md#printing), as the table pandas prints | pandas's text for a complex column and for a frame inside a list, and the terminal's width when printing (idea I.49); `value_counts` and `describe` (3.13.9) |
| [Choosing rows](dataframes.md#choosing-rows): `df[mask]`, `df[i:j]` with a step, `df.head(n)` and `df.tail(n)`, each a new frame, with the panics for a mask of another length and a step of 0; [`for row in df.itertuples()`](dataframes.md#looping-over-rows), also inside `enumerate`, `zip` and a comprehension | Assigning to the rows chosen, `df[mask] = ...`; `itertuples()` kept outside a loop |
| [Choosing and changing columns](dataframes.md#choosing-and-changing-columns): `df[["a", "b"]]`, `df.assign(name=values)`, `df.drop(columns=...)` and `df.rename(columns=...)`, each a new frame whose type the compiler works out, with the panic for a new column of another length | A column name that is not a string literal; `assign` of one value for every row or of a function, and an argument that reads a column added earlier in the same call; `drop` and `rename` in pandas's other forms (`axis=`, `index=`, `inplace=`, a function) |
| [Sorting](dataframe-operations.md#sorting): `df.sort_values(by)` by one column or several, stable, with `ascending` one `bool` or one for each column, and NaN last | pandas's other arguments: `na_position=`, `key=`, `kind=`, `inplace=`, `ignore_index=` |
| [Grouping](dataframe-operations.md#grouping): `df.groupby(keys).agg(name=("column", "reduction"))` by one key column or several, with `sum`, `min`, `max`, `first`, `last`, `count`, `mean`, `var`, `std` and `median`, the keys returned as columns and NaN left out as in pandas | The groups as a value, `g = df.groupby(...)`; `agg` in pandas's other forms (a dict, a function, one name for every column); `df.groupby(...).sum()` and the like; `size`, `nunique`, `transform`, `apply` |
| [Joining](dataframe-operations.md#joining): the inner join `left.merge(right, on=keys)` on one key column or several, with `_x` and `_y` for a column the two frames share | Left, right and outer joins (with missing values, 3.13.6); `left_on=`, `right_on=`, `suffixes=` and pandas's other arguments |
| [Rows that repeat](dataframe-operations.md#rows-that-repeat): `df.drop_duplicates()` and `subset=` with one column or several, the first of the rows that repeat kept; [`a.unique()` and `a.nunique()`](arrays.md#distinct-elements) on a rank-1 array or a column, of numbers or of text; [`df.value_counts("symbol")`](dataframe-operations.md#counting-values) by one column, several or every column, a frame of the values and `count`, the largest count first; [`df.describe()`](dataframe-operations.md#describing-columns), a frame of `statistic` and one column for each column of numbers | `keep="last"` and `keep=False`; `value_counts` with `sort=`, `ascending=`, `normalize=` or `dropna=`; `describe` with `percentiles=`, `include=` or `exclude=` |
| [Stacking frames](dataframe-operations.md#stacking-frames): `concat([a, b])` of any number of frames with one schema | Frames with other columns, filled with missing values (3.13.6); `axis=`, `join=`, `keys=` |

## Errors and panics

Everything in this chapter builds: error types, `Raises`, `raise`, errors passed on through
callers, `try` with `except` naming one type or a tuple of them, `as`, bare `raise`, `main`
ending with an error, and every panic in the table, with its message, location and backtrace.
`ValueError`, `OSError` and `EOFError` build like a program's own error types, and so do `int(text)` and
`float(text)`, which raise `ValueError`. The one exception is the overflow panic: it names the
operator but not yet its operands (roadmap idea I.18), and [Types](#types) lists the operations
it does not check yet.

## Parallel and vector loops

| Builds today | Not built yet |
| --- | --- |
| `prange` with the same arguments as `range`, its inferred reductions and their reproducible combine; `accumulate=` of array variables and of struct fields such as `atoms.forces`, adding into elements and their lanes, and into slices such as `forces[j, :] += f` | |
| `simd(range(...))`, which asks LLVM to vectorize the loop; a float sum or product in one adds into 8 fixed lanes, so it gives the same bits on every machine | A `simd` loop that sums complex numbers runs unvectorized (roadmap idea I.37); an error when LLVM cannot vectorize a `simd` loop, such as one whose bounds checks it cannot move out of the loop (roadmap 3.7); until then such a loop runs unvectorized |

`baryon check` applies every rule of the chapter, so a `prange` loop that breaks one is reported
today, before it can be built.

## Linear algebra and FFT

| Builds today | Not built yet |
| --- | --- |
| [`linalg.solve`](linear-algebra.md#linalgsolve) of an `f64` or `f32` matrix and vector, [`lu_factor` and `lu_solve`](linear-algebra.md#linalglu_factor-and-linalglu_solve), [`det` and `inv`](linear-algebra.md#linalgdet-and-linalginv), [`cholesky`](linear-algebra.md#linalgcholesky), [`qr`](linear-algebra.md#linalgqr), [`eigh` and `eigvalsh`](linear-algebra.md#linalgeigh-and-linalgeigvalsh), [`svd` and `svdvals`](linear-algebra.md#linalgsvd-and-linalgsvdvals), [`eig` and `eigvals`](linear-algebra.md#linalgeig-and-linalgeigvals) of `f64` matrices, every other function at `f64` or `f32`, and [`linalg.LinAlgError`](linear-algebra.md#errors), raised for a singular matrix; [`fft`](linear-algebra.md#fft): `fft`, `ifft`, `rfft`, `irfft`, `fftfreq` and `rfftfreq` of any length, and [`fft2`, `ifft2`, `rfft2` and `irfft2`](linear-algebra.md#fftfft2-and-the-other-two-dimensional-transforms) | Complex matrices and `eig` of an `f32` matrix (idea I.57), `--blas` (3.10.2), several right-hand sides (3.10.3), and `fftn` (idea I.51) |

## Files and processes

| Builds today | Not built yet |
| --- | --- |
| [`sys.argv`](files-and-processes.md#command-line-arguments), a new `list[str]` at each read; [`sys.exit()` and `sys.exit(status)`](files-and-processes.md#exit-status), which panics outside 0 to 255; [`os.getenv`, `os.environ.get`, `os.environ[name]`, `in` and `copy()`](files-and-processes.md#environment-variables); [`OSError` and `os.ErrorKind`](files-and-processes.md#errors-oserror), built, raised, caught and printed; [text files](files-and-processes.md#text-files): `open` in the modes `"r"`, `"w"`, `"a"` and `"x"`, `read`, `readline`, `readlines`, `write`, `flush`, `close`, `with open(...) as f` and `for line in f`, in a loop or a comprehension; [`subprocess.run`](files-and-processes.md#running-a-program) with `capture_output`, `input`, `cwd`, `env` and `timeout`, and `CompletedProcess`; [array files](files-and-processes.md#array-files): `Array.fromfile` with `dtype`, `count` and `offset`, reading numbers, `bool`s, `Fixed` arrays and `Copy` structs of them, and `a.tofile` of any `Array`, a view included; [`pathlib.Path` and the directory functions of `os`](files-and-processes.md#paths-and-directories); [the standard streams](files-and-processes.md#standard-streams): `sys.stdin`, `sys.stdout` and `sys.stderr` as files, `print(..., file=, flush=)`, and `input`, which raises `EOFError` | printing a `Path` inside another value (roadmap 1.13.6); `subprocess.run(check=True)` and `subprocess.Popen`, `Array.fromfile` of a type parameter, and the `.npy` format (idea I.39) |

## Data files

| Builds today | Not built yet |
| --- | --- |
| [Text tables](data-files.md#text-tables): `Array.loadtxt` of `f64` with `dtype`, `comments`, `delimiter`, `skiprows` and `max_rows`, as a table or, where the context needs it, a rank-1 array; `Array.savetxt` of an `Array[f64, 1]` or `Array[f64, 2]` with `delimiter`, `newline`, `header`, `footer` and `comments`; [`.npy` files](data-files.md#npy-files): `Array.load` and `Array.save` of arrays of numbers, `bool`s and complex numbers of any rank, a view included; [FITS files](data-files.md#fits-files): `fits.getdata` of any HDU's image as integers or floats of any rank, scaled as AstroPy scales, `fits.getheader`, `fits.writeto` with a header and `overwrite`, and a `Header`'s `header[key]` typed by the context, `get`, `in` and `header[key] = value` or `(value, comment)`, raising `KeyError`; [NetCDF classic files](data-files.md#netcdf-files): `Dataset(path)`, `ds.variables[name][:]` of a whole variable or a subscript of integers and slices, `var[0, :, ::2]`, as signed integers or floats of any rank, unpacked as `netCDF4` unpacks, `ds.dimensions[name]` with `name`, `size` and `isunlimited()`, `getncattr` typed by the context and `ncattrs` of a file or a variable, raising `KeyError` and `AttributeError`; writing them with `Dataset(path, "w", format=...)` of either classic format or the mode `"a"`, alone or in a `with` block, `createDimension`, `createVariable` of up to four dimensions, `setncattr` of a `str`, `int` or `float`, and `var[:] = values` or a subscript's values, `var[1, ::2] = values`, past the last record too, a subscript that picks nothing raising `IndexError` or `ValueError` as `netCDF4` raises them, of signed integers or floats, packed as `netCDF4` packs, byte for byte as `netCDF4` writes; reading NetCDF-4 files the same way, their variables stored without compression; [HDF5 files](data-files.md#hdf5-files): `h5by.File(path)`, alone or in a `with` block, `f[path]` and `group[path]` as a `Group` or a `Dataset` by their use, `dset[...]` or a subscript of integers and slices as integers or floats of any rank, from every layout, chunk index and the deflate, shuffle and fletcher32 filters, `shape`, `ndim`, `size`, `dtype` and `name`, `attrs[name]` typed by the context, `keys()` and `in`, raising `KeyError`, `IndexError` and `ValueError` as h5py raises them | `loadtxt` of other element types, `usecols`, `unpack`, `converters` and `ndmin`; `savetxt`'s `fmt`; `.npy` files of structs, in Fortran order or big-endian, and `.npz` archives; `fits.open`, FITS tables, compressed images and `fits.Header()`; NetCDF subscripts of index arrays, listing a NetCDF file's variables and dimensions, and NetCDF-4 files' compressed variables, groups, unsigned, 64-bit and string types, and writing; writing HDF5 files and HDF5 datasets of strings or compound types; CSV into a DataFrame and Parquet (roadmap 3.14.4 and 3.14.6) |

## Plotting

| Builds today | Not built yet |
| --- | --- |
| [`byplot`](plotting.md): `plt.axes()`, `Axes.plot` of `Array[f64, 1]`s with `color`, `linewidth`, `linestyle`, `marker`, `markersize` and `label`, `scatter`, `bar`, `hist`, `legend`, `grid`, `imshow` of an `Array[f64, 2]` with matplotlib's common colormaps, `vmin`, `vmax`, `origin`, `aspect` and `extent`, `colorbar`, `set_title`, `set_xlabel`, `set_ylabel`, `set_xlim`, `set_ylim`, [`set_xscale` and `set_yscale`](plotting.md#logarithmic-axes) with `"log"`, [`errorbar`, `fill_between` and `text`](plotting.md#error-bars-filled-areas-and-text), [`plt.subplots`](plotting.md#grids-of-plots) grids written by `Figure.savefig`, [`contour` and `contourf`](plotting.md#contour-plots) of an `Array[f64, 2]`, and `savefig` to an SVG file, a [PNG file](plotting.md#png-files) or a [PDF file](plotting.md#pdf-files); [`ticker`](plotting.md#ticker): `MaxNLocator.tick_values` and `ScalarFormatter.format_ticks` and `get_offset`, `LogLocator.tick_values` and `LogFormatterSciNotation.format_ticks`, with matplotlib's results; the [internal files under `byplot`](plotting.md#the-files-under-byplot) build only inside the standard library | other colormaps, `interpolation`, and images of integers or RGB; `errorbar` and `fill_between` with scalar arguments, `None` for an absent array, and `where=`; `sharex`, `sharey` and `constrained_layout`; a count of contour levels, `clabel` and 2-D `X` and `Y` grids; `savefig(dpi=...)`; plotting lists and integer arrays |

## Compression

| Builds today | Not built yet |
| --- | --- |
| [`zlib.compress`](compression.md#zlibcompress) at every level and in each container; [`zlib.decompress`](compression.md#zlibdecompress) of zlib streams, raw deflate streams and gzip members with every `wbits` form Python accepts, raising `OSError` with Python's message for a corrupt stream; [`zlib.crc32` and `zlib.adler32`](compression.md#zlibcrc32-and-zlibadler32); [`zlib.decompressobj`](compression.md#zlibdecompressobj) with `decompress`, `flush`, `eof` and `unused_data`; [`gzip.compress`](compression.md#gzipcompress) and [`gzip.decompress`](compression.md#gzipdecompress) of one member or several; [`compression.zstd.decompress`](compression.md#compressionzstddecompress) of one frame or several and [`compression.zstd.compress`](compression.md#compressionzstdcompress) at every level, imported with `from compression import zstd` or `from compression.zstd import decompress`; [`bz2.compress`](compression.md#bz2compress) at every level and [`bz2.decompress`](compression.md#bz2decompress) of one stream or several; [`lzma.decompress`](compression.md#lzmadecompress) of xz and `.lzma` files | `zlib.compressobj` and preset dictionaries; `gzip.open` (3.27.4); `import compression.zstd` (idea I.56), Zstandard dictionaries and `options`; bzip2's randomized blocks; `lzma.compress`, `lzma.open` and xz filters before LZMA2 |

## Tests

Everything in this chapter builds: `@test` functions anywhere in the package, tests that raise, and
`baryon test` with its report and exit status.
