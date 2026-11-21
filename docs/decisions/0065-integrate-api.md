# 0065. `integrate` keeps SciPy's calls, with a generic state and `method=` chosen when compiling

Status: Accepted
Date: 2026-11-19
Roadmap: 3.19.1

## Context

Row 3.19 ports `scipy.integrate` under SciPy's names
([decision 0054](0054-scipy-module-names.md)). The fixed rules, which take sampled arrays, are
built (row 3.19.3). The rest takes a function: `quad(f, a, b)` and
`solve_ivp(fun, t_span, y0)`. SciPy's API leans on things Baryon does not have. A method can be
any string at run time, failures can come back as warnings, results are dict-like objects, and
`events=` takes functions with attributes set on them.

A study on main e78e98c (2026-11-14) compiled each shape the port needs. Passing a module-level
function, a tuple of parameters or a trait struct costs nothing measurable, because decision
[0026](0026-functions-as-arguments.md) inlines all three: Lorenz with RK4 ran at 1.03 s for 50
million steps either way, against 1.05 s written by hand. The state type matters far more. The
same RK4 on an `Array[f64, 1]` was 8.4 times slower than on a `Fixed[f64, (3,)]`, because each
`y + h * k` allocates.

The chief designer chose:

- variadic generics for `args=` (card 42), recorded in the 0026 amendment of 2026-11-19;
- the seven API points below (card 43, "Accept all seven", 2026-11-14);
- six details of both records (card 44, "Accept all six", 2026-11-19).

## Decision

### 1. The state is generic

`solve_ivp[S: VectorSpace, *Ts](fun, t_span, y0: S, ...)` takes any state type that has
vector arithmetic. `y0` may be a `Fixed` vector, fast for small systems, or an `Array[f64, 1]`
of any size. Whatever `S` is, `sol.y` is an `Array[f64, 2]` of shape `(n, len(sol.t))`, as in
SciPy.

### 2. `method=` is a string, chosen when compiling when it is a literal

`method=` takes SciPy's names, `"RK45"` and `"DOP853"`.

- **Literal, or left out.** The checker sends the call to the internal function for that
  method, as decision [0064](0064-utf-8-text-from-bytes.md) sends `gzip.open(path, "rt")` to its
  text form. Only that integrator is compiled for each callback. An unknown literal, such as
  `"RK54"`, is a compile error that lists the valid names.
- **Held in a variable.** The method is picked at run time. An unknown name raises
  `ValueError` with SciPy's message, as SciPy does.

A call whose method is a literal, or left out, has no `ValueError` in its `Raises`. Only a
method held in a variable adds `Raises[..., ValueError]` (card 44, point 5).

### 3. Failure follows SciPy where SciPy has a contract, and panics where the program is wrong

- **`solve_ivp`** never raises for a failed integration. As in SciPy, it returns `status = -1`,
  `success = False` and a `message`.
- **`quad`** raises `IntegrationError` with SciPy's message text, for each of the five QUADPACK
  failures where SciPy warns with `IntegrationWarning` and still returns a value:
  - the subdivision limit is reached;
  - roundoff error is detected;
  - the integrand behaves badly;
  - the algorithm does not converge;
  - the integral is probably divergent.

  Baryon has no warnings, and a value with a large `abserr` as the only sign is easy to miss, so
  `quad` returns `Raises[tuple[f64, f64], IntegrationError]`, as `linalg.solve` raises
  `LinAlgError` ([decision 0040](0040-linear-algebra-and-fft.md)) (card 44, point 4).
- **Arguments that make no sense** are a bug in the program. They panic, as wrong shapes do by
  decision 0040, where SciPy raises `ValueError` (card 44, points 3 and 4):
  - `first_step <= 0` or `max_step <= 0` for `solve_ivp`;
  - `limit=0` and other invalid input to `quad`.

  The reference flags each of these differences.
- `rtol` below 2.2e-14 is raised to 2.2e-14, as SciPy does. SciPy also warns, and Baryon does
  not.
- A reversed `t_span` integrates backwards, as in SciPy.

With these rules, `solve_ivp` has no `Raises` in the usual case, and `quad` always has
`Raises[..., IntegrationError]`.

### 4. Results are structs with SciPy's field names

`OdeResult` is a struct with SciPy's 11 fields: `t`, `y`, `sol`, `t_events`, `y_events`,
`nfev`, `njev`, `nlu`, `status`, `message` and `success`. They are read as attributes. SciPy's
result is also a dict, and that form is not built.

`sol` is `OdeSolution | None` (card 44, point 6). It is `None` unless `dense_output=True`, as
in SciPy. `sol.sol(t)` calls the `OdeSolution` through `__call__`
([decision 0058](0058-operators-are-dunder-methods.md), built in row 3.19.8).

### 5. Left for later

- **`events=`.** SciPy takes a list of functions, each with `terminal` and `direction`
  attributes. Version 0 can hold neither a function in a list nor an attribute on a function.
  Row 3.19.9 waits for a card on its shape.
- **`vectorized=`, `jac=` with Radau and BDF, `odeint` with LSODA, and `quad_vec`.** None are in
  row 3.19's text. They are ideas I.64 to I.67.

## Alternatives considered

- **`Array[f64, 1]` as the only state, as in SciPy.** It is simpler, but 8.4 times slower on
  the small systems most ODE code solves.
- **An enum or one function per method, instead of the string.** Either changes every SciPy
  call. A literal gives the same compile-time choice with SciPy's spelling.
- **`quad` returning silently, or with a third value as SciPy's `full_output`.** A silent large
  `abserr` is easy to miss, and a third value changes the shape of every call.
- **Raising `ValueError` for bad arguments, as SciPy does.** Every call to `solve_ivp` would then
  carry a `Raises` for mistakes that a correct program never makes.

## Consequences

- A SciPy call ports line for line: `solve_ivp(lorenz, (0.0, 40.0), y0, args=(10.0, 28.0,
  8.0 / 3.0))` and `quad(f, 0.0, 1.0)` are the same text in both.
- Calls differ from SciPy in four ways, and the reference lists each one:
  - `quad` raises where SciPy warns;
  - bad arguments panic where SciPy raises `ValueError`;
  - results have attributes but are not dicts;
  - `events=` is not built yet.
- Only the integrator a literal names is compiled, so code size grows with the methods a program
  uses, not with every method SciPy has. This matters more for `minimize` in row 3.20, which
  has about ten methods.
