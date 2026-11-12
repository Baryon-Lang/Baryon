//! The `linalg` module (roadmap 3.10, decision 0040), built and run: each function's results
//! against `NumPy`'s for the same input. Random inputs come from `random.Philox`, which draws
//! `NumPy`'s numbers bit for bit, so `NumPy` solves the very same system. Results are printed to 12
//! significant digits, since `LAPACK`, under `NumPy`, adds in its own order.

use super::examples::prints;
use super::{baryon_build, package, panic_report, stderr};
use std::process::Command;

/// `solve` of a system that needs row swaps, of a 1 × 1 system, imported as `linalg.solve`
/// too, and of a random 50 × 50 system; and `LinAlgError("Singular matrix")` for a zero matrix
/// and for one whose elimination meets a zero pivot. `NumPy` prints the same lines, from
///
/// ```python
/// g = numpy.random.Generator(numpy.random.Philox(key=2026))
/// v = g.random(50 * 51).reshape(50, 51)
/// x = numpy.linalg.solve(v[:, 1:], v[:, 0])
/// ```
#[test]
fn solve_matches_numpy() {
    let program = r#"
import linalg
from linalg import LinAlgError, solve
from random import Philox


def random_system(n: int, seed: int) -> tuple[Array[f64, 2], Array[f64, 1]]:
    generator = Philox(seed)
    a = Array.zeros((n, n))
    b = Array.zeros(n)
    for i in range(n):
        b[i] = generator.uniform()
        for j in range(n):
            a[i, j] = generator.uniform()
    return a, b


def show(x: Array[f64, 1]):
    for value in x:
        print(f"{value:.12g}", end=" ")
    print()


def main() -> Raises[None, LinAlgError]:
    pivoting = Array.zeros((4, 4))
    rows = [[0.0, 2.0, 1.0, -1.0], [3.0, -1.0, 0.5, 2.0], [1.0, 1.0, 1.0, 1.0], [-2.0, 4.0, 0.0, 3.0]]
    for i in range(4):
        for j in range(4):
            pivoting[i, j] = rows[i][j]
    show(solve(pivoting, Array([1.0, -2.0, 0.5, 4.0])))
    one = Array.fill((1, 1), 4.0)
    show(linalg.solve(one, Array([2.0])))
    a, b = random_system(50, 2026)
    x = solve(a, b)
    print(f"{x[0]:.12g} {x[25]:.12g} {x[49]:.12g}")
    residual = a @ x - b
    print(residual.dot(residual) < 1e-24)
    try:
        solve(Array.zeros((3, 3)), Array([1.0, 2.0, 3.0]))
    except LinAlgError as error:
        print(error.message)
    singular = Array.fill((2, 2), 1.0)
    try:
        solve(singular, Array([1.0, 2.0]))
    except LinAlgError as error:
        print(error.message)
"#;
    prints(
        "linalg-solve",
        program.trim_start(),
        "-0.964285714286 0.25 0.857142857143 0.357142857143 \n\
         0.5 \n\
         -0.600335590569 2.50291194375 1.72623643638\n\
         True\n\
         Singular matrix\n\
         Singular matrix\n",
    )
    .unwrap();
}

/// `lu_factor` and `lu_solve`, `det` and `inv` of a random 5 × 5 matrix; `det` of a singular
/// matrix, `0.0`, and of an empty one, `1.0`; and `LinAlgError` from `inv` of a singular matrix.
/// `scipy.linalg.lu_factor`, `lu_solve` and `numpy.linalg.det` and `inv` print the same lines
/// for the matrix
///
/// ```python
/// numpy.random.Generator(numpy.random.Philox(key=11)).random(25).reshape(5, 5) - 0.5
/// ```
#[test]
fn lu_det_and_inv_match_numpy_and_scipy() {
    let program = r#"
from linalg import LinAlgError, det, inv, lu_factor, lu_solve
from random import Philox


def show(x: Array[f64, 1]):
    for value in x:
        print(f"{value:.12g}", end=" ")
    print()


def main() -> Raises[None, LinAlgError]:
    generator = Philox(11)
    n = 5
    a = Array.zeros((n, n))
    for i in range(n):
        for j in range(n):
            a[i, j] = generator.uniform() - 0.5
    lu, piv = lu_factor(a)
    print(piv)
    show(lu[n - 1, :])
    show(lu_solve(lu, piv, Array([1.0, 2.0, 3.0, 4.0, 5.0])))
    print(f"{det(a):.12g}", det(Array.fill((3, 3), 2.0)), det(Array.zeros((0, 0))))
    inverse = inv(a)
    show(inverse[0, :])
    show(inverse[:, 4])
    try:
        inv(Array.fill((2, 2), 1.0))
    except LinAlgError as error:
        print(error.message)
"#;
    prints(
        "linalg-lu",
        program.trim_start(),
        "[1 2 4 4 4]\n\
         -0.0212000958787 0.258045423418 0.249879688839 0.494044049809 -0.383324095346 \n\
         7.67510739253 37.1622007148 -23.6703267158 4.86295616996 -4.12915577324 \n\
         0.00429322797388 0.0 1.0\n\
         -4.87824634096 1.99769052254 -3.55898033704 3.13562267707 1.33848459825 \n\
         1.33848459825 4.19714419578 -2.25016069937 -0.281973760125 0.861864295096 \n\
         Singular matrix\n",
    )
    .unwrap();
}

/// `qr` of a tall 6 × 4 and a wide 2 × 3 matrix, with LAPACK's signs, and `cholesky` of the
/// positive definite aᵀa, built with loops; `LinAlgError` from `cholesky` of its negative. `numpy.linalg.qr`
/// and `cholesky` print the same lines for the matrix
///
/// ```python
/// numpy.random.Generator(numpy.random.Philox(key=5)).random(24).reshape(6, 4) - 0.5
/// ```
#[test]
fn qr_and_cholesky_match_numpy() {
    let program = r#"
from linalg import LinAlgError, cholesky, qr
from random import Philox


def show(x: Array[f64, 1]):
    for value in x:
        print(f"{value:.12g}", end=" ")
    print()


def main() -> Raises[None, LinAlgError]:
    generator = Philox(5)
    m = 6
    n = 4
    a = Array.zeros((m, n))
    for i in range(m):
        for j in range(n):
            a[i, j] = generator.uniform() - 0.5
    q, r = qr(a)
    print(q.shape, r.shape)
    show(q[:, 0])
    show(q[m - 1, :])
    show(r[0, :])
    show(r[n - 1, :])
    wide = Array.zeros((2, 3))
    wide[0, 0] = 1.0
    wide[0, 2] = 2.0
    wide[1, 1] = -3.0
    wide[1, 2] = 4.0
    w, s = qr(wide)
    show(w[0, :])
    show(s[1, :])
    spd = Array.zeros((n, n))
    for i in range(n):
        for j in range(n):
            for row in range(m):
                spd[i, j] += a[row, i] * a[row, j]
    l = cholesky(spd)
    show(l[n - 1, :])
    show(l[0, :])
    try:
        cholesky(-spd)
    except LinAlgError as error:
        print(error.message)
"#;
    prints(
        "linalg-qr-cholesky",
        program.trim_start(),
        "(6, 4) (4, 4)\n\
         -0.281522415406 0.302304503291 -0.419073951516 0.537691320677 -0.407090667502 0.445981358553 \n\
         0.445981358553 0.52929388876 0.235715737462 0.276282679904 \n\
         -0.830292519007 -0.218066672763 0.280847721127 -0.145792048279 \n\
         0 0 0 0.461631664317 \n\
         1 0 \n\
         0 -3 4 \n\
         0.145792048279 -0.0976633501946 -0.0665563428344 0.461631664317 \n\
         0.830292519007 0 0 0 \n\
         Matrix is not positive definite\n",
    )
    .unwrap();
}

/// `eigh` of a 6 × 6 matrix that is not symmetric, so only its lower triangle counts, with each
/// eigenvector's sign fixed by its first entry; `eigvalsh` of a 2 × 2 and a 1 × 1 matrix.
/// `numpy.linalg.eigh` and `eigvalsh` print the same lines, after the same sign fix, for the
/// matrix
///
/// ```python
/// numpy.random.Generator(numpy.random.Philox(key=3)).random(36).reshape(6, 6) - 0.5
/// ```
#[test]
fn eigh_matches_numpy() {
    let program = r#"
from linalg import LinAlgError, eigh, eigvalsh
from random import Philox


def show(x: Array[f64, 1]):
    for value in x:
        print(f"{value:.12g}", end=" ")
    print()


def main() -> Raises[None, LinAlgError]:
    generator = Philox(3)
    n = 6
    a = Array.zeros((n, n))
    for i in range(n):
        for j in range(n):
            a[i, j] = generator.uniform() - 0.5
    w, v = eigh(a)
    show(w)
    for column in range(n):
        sign = 1.0
        if v[0, column] < 0.0:
            sign = -1.0
        show(sign * v[:, column])
    pair = Array.fill((2, 2), 1.0)
    pair[0, 0] = 2.0
    pair[1, 1] = 2.0
    show(eigvalsh(pair))
    show(eigvalsh(Array.zeros((1, 1))))
"#;
    prints(
        "linalg-eigh",
        program.trim_start(),
        "-1.09864751102 -0.623056033148 0.105672894405 0.252152380431 0.632048281014 1.39128252921 \n\
         0.0702231302418 -0.259929287153 -0.348399288863 0.511270318189 -0.537915007938 -0.505344950655 \n\
         0.44852342135 0.740061248056 -0.0223712546044 0.422991542099 -0.111334310976 0.243553778672 \n\
         0.390381331872 -0.368034704039 -0.669218587309 -0.0379479424274 0.172656077597 0.482751682839 \n\
         0.444733592095 -0.0733501484867 0.0814411821081 0.15264609584 0.691109147793 -0.537834953214 \n\
         0.394668629067 0.239300732214 -0.189159183759 -0.731164547665 -0.312893929062 -0.344508968513 \n\
         0.536606535087 -0.432031812148 0.622776109002 0.0183941398049 -0.304807364243 0.210489867155 \n\
         1 3 \n\
         0 \n",
    )
    .unwrap();
}

/// The program `svd_matches_numpy` builds, kept apart so the test stays short.
const SVD_PROGRAM: &str = r#"
from linalg import LinAlgError, svd, svdvals
from random import Philox


def show(x: Array[f64, 1]):
    for value in x:
        if abs(value) < 1e-12 * max(1.0, abs(x[0])):
            print(0, end=" ")
        else:
            print(f"{value:.12g}", end=" ")
    print()


def random_matrix(m: int, n: int, seed: int) -> Array[f64, 2]:
    generator = Philox(seed)
    a = Array.zeros((m, n))
    for i in range(m):
        for j in range(n):
            a[i, j] = generator.uniform() - 0.5
    return a


def check(a: Array[f64, 2], full: bool) -> Raises[None, LinAlgError]:
    u, s, vh = svd(a, full)
    print(u.shape, s.shape, vh.shape)
    show(s)
    k = len(s)
    worst = 0.0
    for i in range(a.shape[0]):
        for j in range(a.shape[1]):
            total = 0.0
            for p in range(k):
                total += u[i, p] * s[p] * vh[p, j]
            worst = max(worst, abs(total - a[i, j]))
    orthogonal = 0.0
    for i in range(u.shape[1]):
        for j in range(u.shape[1]):
            total = 0.0
            for r in range(u.shape[0]):
                total += u[r, i] * u[r, j]
            if i == j:
                total -= 1.0
            orthogonal = max(orthogonal, abs(total))
    for i in range(vh.shape[0]):
        for j in range(vh.shape[0]):
            total = 0.0
            for r in range(vh.shape[1]):
                total += vh[i, r] * vh[j, r]
            if i == j:
                total -= 1.0
            orthogonal = max(orthogonal, abs(total))
    print(worst < 1e-13 * max(1.0, s[0]), orthogonal < 1e-13)


def main() -> Raises[None, LinAlgError]:
    tall = random_matrix(6, 4, 21)
    check(tall, True)
    u, s, vh = svd(tall, False)
    for p in range(len(s)):
        sign = 1.0
        if vh[p, 0] < 0.0:
            sign = -1.0
        show(sign * vh[p, :])
        show(sign * u[:, p])
    check(random_matrix(3, 5, 22), True)
    check(random_matrix(5, 5, 23), False)
    check(Array.zeros((3, 2)), True)
    rank_one = Array.zeros((4, 3))
    for i in range(4):
        for j in range(3):
            rank_one[i, j] = (i + 1.0) * (j + 2.0)
    check(rank_one, True)
    graded = Array.zeros((3, 3))
    for i in range(3):
        for j in range(3):
            graded[i, j] = 10.0 ** (-4.0 * (i + j)) * (1.0 + i + j * j)
    show(svdvals(graded))
    hilbert = Array.zeros((8, 8))
    for i in range(8):
        for j in range(8):
            hilbert[i, j] = 1.0 / (i + j + 1.0)
    hilbert_values = svdvals(hilbert)
    for value in hilbert_values:
        print(f"{value:.6g}", end=" ")
    print()
"#;

/// `svd` and `svdvals` of tall, wide and square random matrices, with `full_matrices` both ways,
/// and of hard cases: a zero matrix, a rank-one matrix, a graded matrix whose entries span 16
/// orders of magnitude, and the 8 × 8 Hilbert matrix. Each prints its shapes and singular values,
/// whether `u @ diag(s) @ vh` rebuilds the matrix and `u` and `vh` are orthonormal, and for the
/// tall matrix its singular vectors with signs fixed by `vh`'s first column. `numpy.linalg.svd`
/// prints the same lines; singular values below 10⁻¹² of the largest print as 0, since both
/// libraries leave them as rounding noise, and the Hilbert matrix's to 6 digits, which both get
/// right (the smallest, 1.1115e-10, to 7 digits here and 8 in `LAPACK`).
#[test]
fn svd_matches_numpy() {
    prints("linalg-svd", SVD_PROGRAM.trim_start(), "(6, 6) (4,) (4, 4)\n\
         1.10089323869 0.977431148577 0.512407176324 0.275101757412 \n\
         True True\n\
         0.14782628871 0.767289229171 -0.528375677567 0.332014714313 \n\
         -0.0346936726767 -0.710529946731 0.376706542279 -0.183042495631 -0.534777240705 0.180400866117 \n\
         0.683076578667 0.360958905215 0.61419603486 -0.160867296819 \n\
         0.449109788751 0.0549678273987 -0.225286778221 -0.580585194165 0.147367602471 0.621070386684 \n\
         0.488038851123 -0.464990077345 -0.0770927982111 0.734614870683 \n\
         -0.375928495666 -0.441539738389 -0.81055030065 0.0481267709067 0.0411819927955 0.0521199873096 \n\
         0.522849744992 -0.254480127937 -0.581067185945 -0.569411041397 \n\
         0.472844948388 0.124860129592 -0.260946526084 0.612540524497 -0.507280099115 0.245347753684 \n\
         (3, 3) (3,) (5, 5)\n\
         0.771053197435 0.503185968678 0.378859756316 \n\
         True True\n\
         (5, 5) (5,) (5, 5)\n\
         1.08825882412 0.740224511146 0.452453859564 0.209912396433 0.00214216787447 \n\
         True True\n\
         (3, 3) (2,) (2, 2)\n\
         0 0 \n\
         True True\n\
         (4, 4) (3,) (3, 3)\n\
         29.4957624075 0 0 \n\
         True True\n\
         1.00000004 1.00000006e-08 0 \n\
         1.69594 0.298125 0.0262128 0.00146769 5.43694e-05 1.29433e-06 1.79887e-08 1.11154e-10 \n").unwrap();
}

/// The program `eig_matches_numpy_on_hard_cases` builds, kept apart so the test stays short.
const EIG_PROGRAM: &str = r#"
from linalg import LinAlgError, eig, eigvals


def tidy(x: f64) -> f64:
    if abs(x) < 1e-9:
        return 0.0
    return x


def report(name: str, a: Array[f64, 2]) -> Raises[None, LinAlgError]:
    w, v = eig(a)
    n = a.shape[0]
    worst = 0.0
    for j in range(n):
        for i in range(n):
            total = complex(0.0, 0.0)
            for k in range(n):
                total = total + a[i, k] * v[k, j]
            worst = max(worst, abs(total - w[j] * v[i, j]))
    print(name, worst < 1e-6, end=" ")
    for i in range(1, n):
        current = w[i]
        j = i - 1
        while j >= 0 and (w[j].real > current.real + 1e-9 or (abs(w[j].real - current.real) <= 1e-9 and w[j].imag > current.imag)):
            w[j + 1] = w[j]
            j -= 1
        w[j + 1] = current
    for j in range(n):
        print(f"{tidy(w[j].real):.9g},{tidy(w[j].imag):.9g}", end=" ")
    print()


def main() -> Raises[None, LinAlgError]:
    jordan2 = Array.zeros((2, 2))
    jordan2[0, 0] = 2.0
    jordan2[0, 1] = 1.0
    jordan2[1, 1] = 2.0
    report("jordan2", jordan2)
    jordan4 = Array.zeros((4, 4))
    jordan4[0, 0] = 3.0
    jordan4[0, 1] = 1.0
    jordan4[1, 1] = 3.0
    jordan4[1, 2] = 1.0
    jordan4[2, 2] = 3.0
    jordan4[2, 3] = 1.0
    jordan4[3, 3] = 3.0
    report("jordan4", jordan4)
    scaled = Array.zeros((3, 3))
    scaled[0, 0] = 1.0
    scaled[0, 1] = 10000000000.0
    scaled[1, 0] = 1e-10
    scaled[1, 1] = 1.0
    scaled[1, 2] = 10000000000.0
    scaled[2, 1] = 1e-10
    scaled[2, 2] = 1.0
    report("scaled", scaled)
    graded = Array.zeros((4, 4))
    graded[0, 0] = 1.0
    graded[0, 1] = 0.001
    graded[0, 2] = 1e-06
    graded[0, 3] = 1e-09
    graded[1, 0] = 1000.0
    graded[1, 1] = 2.0
    graded[1, 2] = 0.001
    graded[1, 3] = 1e-06
    graded[2, 0] = 1000000.0
    graded[2, 1] = 1000.0
    graded[2, 2] = 3.0
    graded[2, 3] = 0.001
    graded[3, 0] = 1000000000.0
    graded[3, 1] = 1000000.0
    graded[3, 2] = 1000.0
    graded[3, 3] = 4.0
    report("graded", graded)
    near_repeated = Array.zeros((2, 2))
    near_repeated[0, 0] = 1.0
    near_repeated[0, 1] = 1e-08
    near_repeated[1, 1] = 1.000000000001
    report("near_repeated", near_repeated)
    rotation = Array.zeros((2, 2))
    rotation[0, 1] = -1.0
    rotation[1, 0] = 1.0
    report("rotation", rotation)
    wilkinson10 = Array.zeros((10, 10))
    wilkinson10[0, 0] = 55.0
    wilkinson10[0, 1] = -1320.0
    wilkinson10[0, 2] = 18150.0
    wilkinson10[0, 3] = -157773.0
    wilkinson10[0, 4] = 902055.0
    wilkinson10[0, 5] = -3416930.0
    wilkinson10[0, 6] = 8409500.0
    wilkinson10[0, 7] = -12753576.0
    wilkinson10[0, 8] = 10628640.0
    wilkinson10[0, 9] = -3628800.0
    wilkinson10[1, 0] = 1.0
    wilkinson10[2, 1] = 1.0
    wilkinson10[3, 2] = 1.0
    wilkinson10[4, 3] = 1.0
    wilkinson10[5, 4] = 1.0
    wilkinson10[6, 5] = 1.0
    wilkinson10[7, 6] = 1.0
    wilkinson10[8, 7] = 1.0
    wilkinson10[9, 8] = 1.0
    report("wilkinson10", wilkinson10)
    roots_of_unity = Array.zeros((5, 5))
    roots_of_unity[0, 4] = 1.0
    roots_of_unity[1, 0] = 1.0
    roots_of_unity[2, 1] = 1.0
    roots_of_unity[3, 2] = 1.0
    roots_of_unity[4, 3] = 1.0
    report("roots_of_unity", roots_of_unity)
    cyclic = Array.zeros((3, 3))
    cyclic[0, 2] = 1.0
    cyclic[1, 0] = 1.0
    cyclic[2, 1] = 1.0
    report("cyclic", cyclic)
    zero = Array.zeros((2, 2))
    report("zero", zero)
    one = Array.zeros((1, 1))
    one[0, 0] = 7.5
    report("one", one)
    infinite = Array.fill((2, 2), 1.0)
    infinite[0, 1] = 1e308 * 10.0
    try:
        eigvals(infinite)
    except LinAlgError as error:
        print(error.message)
"#;

/// `eig` of matrices that break simple eigenvalue codes, with `NumPy`'s eigenvalues, sorted, to 9
/// digits: Jordan blocks of sizes 2 and 4, a matrix with entries from 10⁻¹⁰ to 10¹⁰ that needs
/// balancing, a graded matrix, two eigenvalues 10⁻¹² apart, a rotation and the cyclic permutation
/// with complex pairs, the companion matrix of x⁵ - 1, the companion matrix of Wilkinson's
/// polynomial (x - 1)(x - 2)...(x - 10), a zero matrix and a 1 × 1 matrix. Each line also says
/// whether every `a @ v[:, j]` is within 10⁻⁶ of `w[j] * v[:, j]`. An infinite entry raises
/// `LinAlgError` with `NumPy`'s message.
#[test]
fn eig_matches_numpy_on_hard_cases() {
    prints("linalg-eig", EIG_PROGRAM.trim_start(), "jordan2 True 2,0 2,0 \n\
         jordan4 True 3,0 3,0 3,0 3,0 \n\
         scaled True -0.414213562,0 1,0 2.41421356,0 \n\
         graded True 0.296089645,0 1.39227529,0 2.50774871,0 5.80388636,0 \n\
         near_repeated True 1,0 1,0 \n\
         rotation True 0,-1 0,1 \n\
         wilkinson10 True 1,0 2,0 3,0 4,0 5,0 6,0 7,0 8,0 9,0 10,0 \n\
         roots_of_unity True -0.809016994,-0.587785252 -0.809016994,0.587785252 0.309016994,-0.951056516 0.309016994,0.951056516 1,0 \n\
         cyclic True -0.5,-0.866025404 -0.5,0.866025404 1,0 \n\
         zero True 0,0 0,0 \n\
         one True 7.5,0 \n\
         Array must not contain infs or NaNs\n").unwrap();
}

/// A right-hand side whose length is not the matrix's panics, as mismatched shapes in
/// whole-array arithmetic do, pointing into the library and naming the caller's line.
#[test]
fn solve_with_a_short_right_hand_side_panics() {
    let program = "\
from linalg import LinAlgError, solve


def main() -> Raises[None, LinAlgError]:
    print(solve(Array.zeros((3, 3)), Array.zeros(2)))
";
    let directory = package("linalg-solve-short", &[("main.byn", program)]).unwrap();
    let (output, executable) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&executable).output().unwrap();
    assert_eq!(ran.status.code(), Some(101));
    assert_eq!(
        panic_report(&ran),
        "panic: assertion failed: len(b) == a.shape[0], \"solve needs b as long as a has rows\"\n  \
         --> <std>/linalg.byn:93:5\n"
    );
}

/// The program `f32_matrices_match_numpy` builds, kept apart so the test stays short.
const F32_PROGRAM: &str = r#"
from linalg import LinAlgError, cholesky, det, eigvalsh, inv, qr, solve, svdvals
from random import Philox


def show1(x: Array[f32, 1]):
    for value in x:
        print(f"{value:.4g}", end=" ")
    print()


def show(x: Array[f32, 2]):
    for i in range(x.shape[0]):
        for j in range(x.shape[1]):
            print(f"{x[i, j]:.4g}", end=" ")
    print()


def main() -> Raises[None, LinAlgError]:
    generator = Philox(5)
    n = 4
    a = Array.zeros((n, n), f32)
    b = Array.zeros(n, f32)
    for i in range(n):
        b[i] = f32(generator.uniform())
        for j in range(n):
            a[i, j] = f32(generator.uniform())
    show1(solve(a, b))
    print(f"{det(a):.4g}")
    show(inv(a))
    q, r = qr(a)
    show(q)
    show(r)
    s = Array.zeros((n, n), f32)
    for i in range(n):
        for j in range(n):
            for k in range(n):
                s[i, j] += a[k, i] * a[k, j]
    show(cholesky(s))
    show1(eigvalsh(s))
    show1(svdvals(a))
"#;

/// `solve`, `det`, `inv`, `qr`, `cholesky`, `eigvalsh` and `svdvals` of `f32` matrices (roadmap
/// 3.10.1): each function is generic over its float type, so an `f32` matrix gives an `f32`
/// result computed in `f32`. `NumPy` prints the same lines, to 4 significant digits, for the same
/// `float32` matrices, `g = numpy.random.Generator(numpy.random.Philox(key=5))` drawn in the
/// same order, and `s = a.T @ a`.
#[test]
fn f32_matrices_match_numpy() {
    prints("linalg-f32", F32_PROGRAM.trim_start(), "1.715 -1.853 1.084 -1.504 \n\
         -0.03239\n\
         0.5753 2.809 2.154 -3.659 -6.048 3.921 2.358 -1.356 5.083 -4.739 -4.037 4.286 -1.351 -1.495 0.1128 2.176 \n\
         -0.3998 0.2507 0.7551 -0.4551 -0.5033 -0.5951 -0.3725 -0.5038 -0.5149 0.7064 -0.4842 0.038 -0.5673 -0.2899 0.2378 0.7332 \n\
         -1.477 -0.87 -1.195 -0.9331 0 -0.5583 -0.4876 0.4793 0 0 0.1166 -0.1203 0 0 0 0.3369 \n\
         1.477 0 0 0 0.87 0.5583 0 0 1.195 0.4876 0.1166 0 0.9331 -0.4793 -0.1203 0.3369 \n\
         0.006688 0.03793 0.7749 5.338 \n\
         2.31 0.8803 0.1948 0.08178 \n").unwrap();
}

/// A library function is compiled only into a program that calls it: a program that never
/// calls `solve` has no `solve` in its module, so the library costs it nothing. `solve` is
/// generic over its float type, so the program that calls it gets its `f64` instance.
#[test]
fn library_functions_are_compiled_only_when_called() {
    let compiled = |program: &str| {
        let mut session = driver::Session::new();
        session.add_file("main.byn", program).unwrap();
        session.compile(None, driver::Profile::Debug).unwrap()
    };
    let unused = compiled("def main():\n    print(1)\n");
    assert!(!unused.contains("<std>.linalg.solve"), "{unused}");
    let used = compiled(
        "from linalg import LinAlgError, solve\n\n\n\
         def main() -> Raises[None, LinAlgError]:\n    print(solve(Array.fill((1, 1), 2.0), Array([1.0])))\n",
    );
    assert!(used.contains("@\"<std>.linalg.solve[f64]\""), "{used}");
}

/// The worked example of `linalg.solve` in the reference's Linear algebra chapter, read from
/// the chapter itself so the two cannot drift apart, prints what the chapter says it prints.
#[test]
fn the_reference_example_of_solve_runs() {
    let chapter = include_str!("../../../../docs/reference/linear-algebra.md");
    let section = &chapter[chapter.find("## `linalg.solve`").unwrap()..];
    let program = section
        .split("```python\n")
        .nth(2)
        .and_then(|block| block.split("```").next())
        .unwrap();
    let printed = section
        .split("```text\n")
        .nth(1)
        .and_then(|block| block.split("```").next())
        .unwrap();
    prints("linalg-reference-solve", program, printed).unwrap();
}

/// The worked example in the reference's `eig` section, the eigenvalues of a spring and the
/// singular values of a tall matrix, builds and prints what the chapter says it prints.
#[test]
fn the_reference_example_of_eig_runs() {
    let chapter = include_str!("../../../../docs/reference/linear-algebra.md");
    let section = &chapter[chapter.find("## `linalg.eig`").unwrap()..];
    let program = section
        .split("```python\n")
        .nth(2)
        .and_then(|block| block.split("```").next())
        .unwrap();
    let printed = section
        .split("```text\n")
        .nth(1)
        .and_then(|block| block.split("```").next())
        .unwrap();
    prints("linalg-reference-eig", program, printed).unwrap();
}
