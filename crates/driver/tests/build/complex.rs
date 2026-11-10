//! Complex numbers built and run: literals, the operators, `complex(re, im)`, `real`, `imag`,
//! `conjugate()`, `abs`, `**` and `print`. Every `complex128` line is what CPython prints for
//! the same program; division by zero and zero to a negative power give `NumPy`'s IEEE results,
//! where Python raises; the `complex64` line is what `NumPy` prints for `numpy.complex64`.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// One program, every operation, compared with Python's and `NumPy`'s output.
#[test]
fn complex_numbers_build_and_run_as_python_computes_them() {
    let program = "\
class Wave(Struct, Copy):
    amplitude: complex
    k: f64


def rotate(z: complex, t: f64) -> complex:
    return z * complex(0.0, t)


def main():
    z = 1.5 + 2j
    w = complex(3, -4)
    print(z, w, -2j, 2j, 0j, z.real, z.imag, abs(w))
    print(z + w, z - w, z * w, z / w, z == w, z != w, -z)
    print(z.conjugate(), z ** 2, z ** 0.5, w ** z, 1j ** 1j)
    print(2 * z, z / 2, 1 / z, 1.5 - z)
    u = z
    u += 1
    u *= 1j
    u **= 3
    wave = Wave(amplitude=1 - 1j, k=2.0)
    print(u, (z * wave.amplitude).real, rotate(z, 2.0), complex(1e300, 1e300) / complex(1e300, 1e300))
    print((1 + 1j) / 0, 1j / 0, 0j ** -1)
    a: complex64 = complex64(0.1, 0.2)
    print(a, a * a, abs(a), a.real, a ** 2)
";
    let directory = package("complex", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(stderr(&ran), "");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "\
(1.5+2j) (3-4j) (-0-2j) 2j 0j 1.5 2.0 5.0
(4.5-2j) (-1.5+6j) (12.5+0j) (-0.14+0.48j) False True (-1.5-2j)
(1.5-2j) (-1.75+6j) (1.4142135623730951+0.7071067811865476j) \
(-18.166068671965473+69.08356110951415j) (0.20787957635076193+0j)
(3+4j) (0.75+1j) (0.24-0.32j) -2j
(29.5+14.375j) 3.5 (-4+3j) (1+0j)
(inf+infj) (nan+infj) (nan+nanj)
(0.1+0.2j) (-0.030000001+0.040000003j) 0.2236068 0.1 (-0.030000001+0.040000003j)
"
    );
}

/// Complex numbers in f-strings: the empty specification as `print` writes them, a type letter
/// on each part without parentheses, a width on the whole, `#` keeping every point, and the reference's worked example.
/// Every line is what CPython writes for the same f-strings.
#[test]
fn complex_numbers_format_in_f_strings_as_python_does() {
    let program = "\
def impedance(resistance: f64, reactance: f64) -> str:
    z = complex(resistance, reactance)
    return f\"Z = {z:.1f} ohm, |Z| = {abs(z):.2f}\"


def main():
    z = complex(1.0, -2.5)
    w = complex64(complex(0.1, 0.2))
    print(f\"{z} {z:.2f} [{z:>14}] {2j} {w} {z:+.3e} {z:#} {z:#.0f}\")
    print(impedance(3.0, -4.0))
";
    super::examples::prints(
        "complex-format",
        program,
        "(1-2.5j) 1.00-2.50j [      (1-2.5j)] 2j (0.1+0.2j) +1.000e+00-2.500e+00j (1.-2.5j) 1.-2.j\nZ = 3.0-4.0j ohm, |Z| = 5.00\n",
    )
    .unwrap();
}
