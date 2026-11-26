//! `lu_factor`, `lu_solve`, `solve`, `det`, `inv`, `cholesky` and `qr` of `complex128` matrices
//! (roadmap 3.10.8 and 3.10.9):
//! the overloads beside the real functions (decision 0067) give the results of `NumPy` and `SciPy`.

use crate::examples::prints;

/// A random 4 × 4 complex system from `random.Philox(41)`, each entry's real part drawn before
/// its imaginary part, row by row, with `b`'s two columns after `a`'s row: `solve` of the matrix
/// `b`, `lu_factor`'s pivots, `lu_solve` of `b`'s second column, `det`, `inv`'s first row, and
/// a singular matrix, whose `det` is `0j` and whose `inv` raises. The expected text is `NumPy`
/// 2.5.3's and `SciPy` 1.18.1's, printed the same way:
///
/// ```python
/// g = numpy.random.Generator(numpy.random.Philox(key=41))
/// numpy.linalg.solve(a, b); scipy.linalg.lu_factor(a)[1]; numpy.linalg.det(a)
/// numpy.linalg.inv(a)[0]
/// ```
#[test]
fn complex_matrices_match_numpy() {
    let program = r#"
from linalg import LinAlgError, det, inv, lu_factor, lu_solve, solve
from random import Philox


def show(x: Array[complex128, 1]):
    for i in range(len(x)):
        print(f"{x[i].real:.12g},{x[i].imag:.12g}", end=" ")
    print()


def main() -> Raises[None, LinAlgError]:
    generator = Philox(41)
    a = Array.zeros((4, 4), complex128)
    b = Array.zeros((4, 2), complex128)
    for i in range(4):
        for j in range(4):
            a[i, j] = complex(generator.uniform(), generator.uniform())
        for j in range(2):
            b[i, j] = complex(generator.uniform(), generator.uniform())
    x = solve(a, b)
    for j in range(2):
        column = Array.zeros(4, complex128)
        for i in range(4):
            column[i] = x[i, j]
        show(column)
    lu, piv = lu_factor(a)
    print(piv)
    column = Array.zeros(4, complex128)
    for i in range(4):
        column[i] = b[i, 1]
    show(lu_solve(lu, piv, column))
    d = det(a)
    print(f"{d.real:.12g},{d.imag:.12g}")
    ai = inv(a)
    row = Array.zeros(4, complex128)
    for j in range(4):
        row[j] = ai[0, j]
    show(row)
    singular = Array.zeros((2, 2), complex128)
    singular[0, 0] = 1 + 1j
    singular[0, 1] = 2 + 2j
    singular[1, 0] = 2 + 2j
    singular[1, 1] = 4 + 4j
    print(det(singular))
    try:
        print(inv(singular))
    except LinAlgError as error:
        print("inv:", error.message)
"#;
    prints(
        "linalg-complex",
        program.trim_start(),
        "-1.45738681993,2.47784167062 1.83596152446,-0.193021566437 0.288722608243,-0.00262709748963 -0.640615877396,-2.49656756502 \n\
         1.04724228373,-0.521385247845 -0.532335786937,0.516899663757 -0.357015658114,-0.347327615615 0.707307506716,0.174685548242 \n\
         [2 2 3 3]\n\
         1.04724228373,-0.521385247845 -0.532335786937,0.516899663757 -0.357015658114,-0.347327615615 0.707307506716,0.174685548242 \n\
         0.388572308033,0.0489959502545\n\
         1.06329686967,0.447233116153 -1.343547501,1.41815944262 -0.118101990998,-1.52486295864 0.974912174508,0.579294718843 \n\
         0j\n\
         inv: Singular matrix\n",
    )
    .unwrap();
}

/// `cholesky` of a Hermitian positive definite matrix, `b @ bᴴ + 3I` with `b` from
/// `random.Philox(43)`, and `qr` of a 4 × 3 matrix drawn after it and of its 3 × 4 transpose,
/// against `numpy.linalg.cholesky` and `numpy.linalg.qr`: `q` and `r` match `NumPy`'s with their
/// phases, not only up to them, since the reflections are made as LAPACK's `zlarfg` makes them.
/// `cholesky` of the negated matrix raises.
#[test]
fn complex_cholesky_and_qr_match_numpy() {
    let program = r#"
from linalg import LinAlgError, cholesky, qr
from random import Philox


def show(x: Array[complex128, 2]):
    for i in range(x.shape[0]):
        for j in range(x.shape[1]):
            print(f"{x[i, j].real:.12g},{x[i, j].imag:.12g}", end=" ")
    print()


def main() -> Raises[None, LinAlgError]:
    generator = Philox(43)
    b = Array.zeros((3, 3), complex128)
    tall = Array.zeros((4, 3), complex128)
    for i in range(3):
        for j in range(3):
            b[i, j] = complex(generator.uniform(), generator.uniform())
    for i in range(4):
        for j in range(3):
            tall[i, j] = complex(generator.uniform(), generator.uniform())
    a = Array.zeros((3, 3), complex128)
    for i in range(3):
        for j in range(3):
            total = 0j
            for k in range(3):
                total += b[i, k] * b[j, k].conjugate()
            a[i, j] = total
        a[i, i] += 3 + 0j
    show(cholesky(a))
    q, r = qr(tall)
    show(q)
    show(r)
    wide = Array.zeros((3, 4), complex128)
    for i in range(3):
        for j in range(4):
            wide[i, j] = tall[j, i]
    q, r = qr(wide)
    show(q)
    show(r)
    try:
        print(cholesky(-a))
    except LinAlgError as error:
        print("cholesky:", error.message)
"#;
    prints(
        "linalg-complex-qr",
        program.trim_start(),
        "2.28533364176,0 0,0 0,0 0.687442371509,-0.317193520347 2.03240074798,0 0,0 0.772816341266,-0.285444842322 0.416339531461,-0.0674366936802 2.0738094819,0 \n\
         -0.487174181506,-0.380483761549 0.522014911808,0.119374401909 -0.247746055162,-0.32859574539 -0.0396369068203,-0.165060573887 -0.0771095477309,-0.434765341173 -0.311280806568,-0.495183521328 -0.149643168652,-0.592104499144 -0.476677018883,0.0763785099153 -0.207639897281,0.152353171146 -0.374839855365,-0.274939290819 -0.155650248852,-0.510883296563 0.628911350684,0.163367576989 \n\
         -1.55052541014,0 -1.03541615929,0.130167103426 -1.82835737681,0.133261576299 0,0 -1.04215162677,0 -0.314367152705,0.045636536115 0,0 0,0 -0.780161937159,0 \n\
         -0.463536213087,-0.362022472997 0.325854750063,0.350682478078 0.450847051946,0.470798603846 -0.00609710025688,-0.126496982803 -0.286936535519,-0.794371309652 0.230401102044,0.466388591062 -0.592271506168,-0.535956817191 -0.0992835027072,-0.218231332549 -0.384158129629,-0.396112554852 \n\
         -1.62959425016,0 -0.863518479029,-0.360955923577 -1.38208495983,-0.430178432356 -0.960904061103,-0.0386091002376 0,0 -0.63875054256,0 -0.481448891839,0.704296048944 -0.574238297751,0.198948131804 0,0 0,0 0.334880883279,0 0.649829724224,-0.241450032561 \n\
         cholesky: Matrix is not positive definite\n",
    )
    .unwrap();
}
