# Baseline benchmark suite

NPL's performance goal is concrete: numeric code within 5% of `gfortran -O3`. This directory holds
the yardstick. Eight physics and math kernels are written twice, once in C++ and once in Fortran,
with the same algorithm, the same input data and the same order of floating-point operations. A
harness builds them, runs them, checks that both languages computed the same answer, and records
the times as a per-machine baseline.

When the NPL compiler can build these kernels, an NPL version of each joins the suite, and the
baselines here are the numbers it must match. Roadmap item 1.18 turns that comparison into a CI
gate. The reasoning behind the suite's shape is in
[decision record 0003](../docs/decisions/0003-benchmark-suite.md).

## Quick start

You need the pinned Rust toolchain (it installs itself), a C++ compiler and a Fortran compiler.

| Platform | Install |
| --- | --- |
| macOS (Apple silicon) | `xcode-select --install` for `clang++`; `brew install gcc` for `gfortran` |
| Ubuntu or Debian | `sudo apt install clang gfortran` |

```sh
cargo kernels --size smoke     # seconds: does every kernel build, run and agree?
cargo kernels                  # a few minutes: time every kernel at the reference size
cargo kernels --record         # the same, then write bench/baselines/<os>-<arch>.json
```

Homebrew installs the Fortran compiler with a version suffix (`gfortran-14`), so on macOS name it:

```sh
FC=gfortran-14 cargo kernels --record
```

Typical output (Linux x86-64 in a shared cloud VM; your numbers will differ):

```text
kernel                        C++      Fortran     C++/F  checksum
nbody                      0.9139       0.5898      1.55  identical
lennard_jones              4.0736       3.9299      1.04  identical
heat_stencil               0.7571       0.7651      0.99  identical
conjugate_gradient         0.7838       0.8895      0.88  identical
runge_kutta                0.4277       0.6066      0.71  identical
monte_carlo                0.7300       0.7117      1.03  identical
matmul                     0.4469       0.4013      1.11  identical
fft                        1.0374       1.0276      1.01  identical
```

Times are the fastest of the runs, in seconds. `C++/F` is the C++ time divided by the Fortran
time: above 1 means Fortran was faster. `checksum` says whether both languages produced the same
result: `identical` to the last bit, `agree` within the kernel's tolerance, or `DISAGREE`, which
fails the run.

## Options

| Option | Effect |
| --- | --- |
| `--size smoke` | Tiny problems that finish in milliseconds. Proves correctness; the times mean nothing. |
| `--size reference` | The default. Problems that take roughly 0.3 to 3 seconds each. |
| `--repetitions <n>` | Runs per kernel and language. Default 5, or 1 for `smoke`. |
| `--only <a,b,...>` | Run only the named kernels. |
| `--record` | Write the results to `bench/baselines/<os>-<arch>.json`. |

| Environment variable | Meaning | Default |
| --- | --- | --- |
| `CXX` | C++ compiler | `clang++` |
| `CXXFLAGS` | Extra C++ flags, appended after `-O3 -std=c++20` | none |
| `FC` | Fortran compiler | `gfortran` |
| `FFLAGS` | Extra Fortran flags, appended after `-O3 -std=f2018` | none |

Baselines use plain `-O3`, as the design plan specifies, so they are portable across machines of
the same architecture. To see what the hardware can do, add `-march=native` (x86) or
`-mcpu=native` (Apple silicon) through `CXXFLAGS` and `FFLAGS`; the flags are recorded in the
baseline, so the two kinds of run are never confused.

## The kernels

Each kernel is a workload the design plan names, and each stresses something different in a
compiler. The reference size is what `--record` measures.

| Kernel | What it computes | What it stresses | Reference size | Checksum |
| --- | --- | --- | --- | --- |
| [`nbody`](kernels/nbody) | Gravity between every pair of bodies, kick-drift-kick leapfrog | O(n²) loop, square root, vectorized reduction | 4,096 bodies, 20 steps | Total energy |
| [`lennard_jones`](kernels/lennard_jones) | A Lennard-Jones liquid in a periodic box, cell list, velocity Verlet | Indirect memory access, cutoff branch, minimum image | 4,000 atoms, 200 steps | Total energy per atom |
| [`heat_stencil`](kernels/heat_stencil) | 3D heat equation, explicit 7-point Jacobi stencil | Memory bandwidth, stencil vectorization, aliasing | 200³ grid, 100 steps | Sum of the grid |
| [`conjugate_gradient`](kernels/conjugate_gradient) | Conjugate gradient on the 3D Poisson matrix in CSR form | Sparse matrix-vector products, dot products | 96³ unknowns, 100 iterations | Norm of the solution |
| [`runge_kutta`](kernels/runge_kutta) | RK4 on an ensemble of stiff Van der Pol oscillators (μ = 10) | Vectorization across independent systems | 8,192 oscillators, 20,000 steps | Sum of x + y |
| [`monte_carlo`](kernels/monte_carlo) | Monte Carlo integral of exp(−\|x\|²) over the 6D unit cube | Serial random stream, `exp`, long reduction | 50 million samples | The estimate (exact: 0.173504...) |
| [`matmul`](kernels/matmul) | Dense C = AB with plain loops, no BLAS | Floating-point throughput, cache reuse | 1,024 × 1,024 | Sum of C |
| [`fft`](kernels/fft) | Iterative radix-2 complex FFT | Changing strides, bit reversal, complex arithmetic | 2²⁰ points, 10 transforms | Sum of magnitudes |

Every source file opens with a header comment giving its usage, what it stresses, its checksum and
any non-obvious choice.

### Rules every kernel follows

These rules make the two languages comparable and keep the comparison honest. A new kernel must
follow them too.

- **Same algorithm, same operations.** The C++ and Fortran versions are line-for-line twins: same
  loop structure, same summation order, same constants. Each uses its language's natural memory
  layout and walks memory contiguously in its innermost loop (row-major `i, k, j` in C++,
  column-major `j, k, i` in Fortran for `matmul`).
- **Plain code, no libraries.** No BLAS, FFTW, `matmul` intrinsic, OpenMP or hand-written SIMD.
  The suite measures what a compiler does with straightforward code, which is what NPL's compiler
  will be judged on. Library comparisons belong to roadmap item 3.10.
- **Same input in both languages.** Inputs come from one generator, xorshift64 with a fixed seed,
  implemented in [`support.hpp`](kernels/support/support.hpp) and
  [`support.f90`](kernels/support/support.f90). It uses only shifts and exclusive-or, which both
  languages define identically for 64-bit integers.
- **Only the computation is timed.** Input generation, matrix assembly, twiddle factors and the
  checksum are outside the timed region, measured with a monotonic clock.
- **Single-threaded and IEEE.** No `-ffast-math`, no threads. Parallel baselines are a separate,
  later item.
- **Complex arithmetic spelled out.** `fft` stores real and imaginary parts in separate arrays and
  multiplies by hand, because C++'s `std::complex` adds NaN-recovery branches that Fortran's
  `complex` does not.

### Known asymmetries

Two differences between the languages are left in on purpose, because they are exactly what NPL
is designed to win on:

- **Aliasing.** Fortran assumes array arguments never overlap; C++ must assume they might, so
  before vectorizing a loop like `heat_stencil`'s the C++ compiler has to prove, or check at run
  time, that input and output are separate. NPL arrays never alias, so NPL should match Fortran
  here.
- **Bounds checks.** Neither baseline checks array bounds. NPL plans to keep bounds checks in
  release builds and hoist them out of loops (roadmap 3.2, idea I.4), so these kernels show what
  that must cost: nothing measurable.

## Correctness check

Each kernel prints a checksum. The harness fails if the C++ and Fortran checksums differ by more
than the kernel's relative tolerance: `1e-9` for most kernels, `1e-5` for `lennard_jones`.
Molecular dynamics is chaotic, so a one-bit difference (for example, a compiler fusing a multiply
and add) grows over the run; its checksum is the total energy, which the integrator conserves to
about `1e-5` whatever the trajectory. With clang++ or g++ and gfortran on x86-64, every checksum
is identical to the last bit.

## Output protocol

A kernel is an executable that takes positive integer arguments and prints exactly two lines:

```text
checksum -4.58485193057788398e-01
seconds 7.06986239999991994e-01
```

Any spacing and either exponent letter is accepted (Fortran prints `E+000`). A wrong argument
prints a usage line to standard error and exits with status 2. The NPL kernels will follow the same
protocol, so the harness needs no change to measure them.

## Baseline files

`--record` writes `bench/baselines/<os>-<architecture>.json`, one file per platform:
`macos-aarch64.json` for Apple silicon, the primary target, and `linux-x86_64.json` for CI's
platform. Recording again on the same platform replaces the file; commit it to update the baseline.

```json
{
  "schema": 1,
  "recorded": "2026-11-06T05:30:00Z",
  "commit": "<the commit the kernels were built from>",
  "size": "reference",
  "repetitions": 5,
  "machine": { "os": "macos", "architecture": "aarch64", "processor": "Apple M3 Pro", "logical_cores": 12 },
  "compilers": {
    "cpp": { "command": "clang++", "version": "...", "flags": ["-O3", "-std=c++20"] },
    "fortran": { "command": "gfortran-14", "version": "...", "flags": ["-O3", "-std=f2018"] }
  },
  "kernels": [
    {
      "name": "nbody",
      "summary": "All-pairs gravity, leapfrog",
      "arguments": ["bodies", "steps"],
      "parameters": [4096, 20],
      "results": {
        "cpp": { "checksum": -0.4584851930577884, "seconds": { "minimum": 1.04, "median": 1.05, "runs": [1.04, 1.06] } },
        "fortran": { "checksum": -0.4584851930577884, "seconds": { "minimum": 0.70, "median": 0.71, "runs": [0.70, 0.72] } }
      }
    }
  ]
}
```

Keys appear in alphabetical order in the written file. `schema` changes whenever the format does.
Compare baselines by `minimum`: the fastest run is the best estimate of the cost on a quiet
machine, and the least affected by other work on the computer.

## Layout

| Path | Contents |
| --- | --- |
| `kernels/<name>/<name>.cpp`, `<name>.f90` | One kernel in each language |
| `kernels/support/` | The shared generator, timer, argument parsing and report, in each language |
| `src/` | The harness: `cargo kernels` |
| `baselines/` | Recorded baselines, one JSON file per platform |

Builds go to `target/bench/`, which Git ignores.

## Adding a kernel

1. Write `kernels/<name>/<name>.cpp` and `kernels/<name>/<name>.f90`, following the rules above and
   the header-comment style of the existing kernels.
2. Add an entry to `KERNELS` in [`src/suite.rs`](src/suite.rs) with its arguments, smoke and
   reference sizes and tolerance. A unit test fails if either source file is missing.
3. Run `cargo kernels --size smoke --only <name>` until both languages agree, then size the
   reference problem to take 0.3 to 3 seconds.
4. Add a row to the kernel table above, and re-record the baselines you can.
