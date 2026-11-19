//! How close each special function must come to its reference: the tolerance in units in the
//! last place (ULP) and the magnitude the error is measured against, for [`super::check`]. They
//! live apart from the programs and fixtures of [`super`] so that each file stays short.

/// How far a value may be from the reference, in ULP of `scale(name, want, x)`, for a case
/// whose first argument is `first`.
///
/// Most functions are relative: within 8 ULP of the reference. The exponential, sine, cosine and
/// Fresnel integrals, zeta and Spence's function are held to 16: each is a series or a continued
/// fraction of many terms. The Bessel functions are held to 16, since each is a sum or a recurrence
/// of many terms, and `yn` to 32: its upward recurrence adds about half a ULP at each order, and
/// the cases reach order 30. `beta` is held to 8 ULP plus 2 for each unit of the logarithm of the
/// value: above 171 it is `exp(betaln)`, and the exponential turns each ULP of the logarithm's
/// absolute error into its size in relative error. `binom` and `comb` are held to the same: past
/// 1,000 factors they are 1/beta, and a product of hundreds of factors rounds once for each.
///
/// The incomplete gamma and beta functions are an exponential of a sum of logarithms times a
/// series, so they too are held to a number of ULP for each unit of the value's logarithm, 16
/// for the gamma functions and 32 for the beta functions, whose sum has three terms. That is
/// divided by the shape `a` when it is below 1: a small shape raises the value to the power
/// 1/a in the inverses and makes the complement 1 minus a number near 1, so each ULP of input
/// becomes 1/a of output. The smallest shape in the cases is 0.001. The distribution functions
/// are the same functions after a change of variables, and are held to 32 ULP for each unit of
/// the logarithm; their first argument is a count or degrees of freedom, not a shape.
///
/// The elliptic integrals are held to the default 8 ULP: Carlson's duplication converges to the
/// last digit. Jacobi's elliptic functions are held to 128: near m = 1 the first of the descending
/// Landen steps takes the arcsine of a number near 1, which multiplies its rounding error, and
/// reaches 96 ULP of the amplitude at u = 20 and m = 1 − 4e-11, where `SciPy` is 10 billion ULP
/// off.
///
/// The orthogonal polynomials and spherical harmonics are their three-term recurrences, which
/// add a few ULP of the largest value they pass through at each degree, so they are held to 8 ULP
/// for each degree up to the first argument, n, and `lpmv`, whose first argument is the order, to
/// 64; the worst is about 4 ULP for each degree, for `sph_harm_y` at degree 46.
///
/// The Bessel functions of real order are held to 256 ULP of the size their case records: past
/// the turning point x = |v| the modulus √(J² + Y²), the envelope J and Y oscillate under, and
/// elsewhere the value itself. The worst is 106 ULP, at orders above 200, where the
/// recurrences of Steed's method run for about v steps.
///
/// The Airy functions are held to 64 ULP, the Kelvin functions to 32 and the Struve functions to
/// 128, of the size their case records: for Airy below 0, the envelope √(Ai² + Bi²) or
/// √(Ai′² + Bi′²); for each Kelvin function, the size of the complex value it is a part of; for
/// H past the turning point x = |v|, the larger of H and the modulus √(J² + Y²), since H is Y
/// plus a smooth part there; and elsewhere the value itself. The worst are 36, 13 and 57 ULP.
///
/// The hypergeometric functions are held to the size their case records: the largest of |f| and
/// |t ∂f/∂t| for each argument t, the error that rounding the arguments alone would make. `hyp0f1`
/// is held to 16 ULP of it, `hyp1f1` to 64 and `hyp2f1` to 128; the worst are 13, 55 and 68.
/// `hyperu` is held to 512: for b below about −30 and |a| below 1 the series it is summed from
/// cancel by up to 64 times, and the worst is 385 ULP there, at b = −146.
///
/// The spheroidal angular functions are held to 16 ULP of the size their case records, and their
/// characteristic values to 64: λ is an eigenvalue of a matrix whose entries reach about c²/2,
/// so for prolate c above about 100, where λ is near c(2(n − m) + 1), the bisection resolves it
/// only to about c²/(2λ) ULP. The worst are 8 and 22 ULP. The radial functions are held to 128:
/// the second kind inside its series' reach is carried by Taylor steps, each of which rounds by
/// about a ULP of its modulus, and the worst is 54 ULP, for oblate c near 30 at ξ = 0.
///
/// The nodes of Gauss quadrature, `roots_..._x`, are held to 16 ULP of the larger of themselves and
/// the gap to the nearest node: Newton's method on the polynomial leaves 7 at worst. Their weights,
/// `roots_..._w`, are held to 32 ULP for each node of the rule, the first argument, n: a weight is
/// exact for the node as rounded to `f64`, and near the end of the interval it changes fast, so one
/// ULP of a node moves it by about 1/(1 − |x|) ULP, and the outermost node of a rule lies closer to
/// the end the more nodes it has. The worst is 19 ULP for each node, at n = 150, where `SciPy`'s
/// weights are off by 1.2 million ULP.
pub(super) fn tolerance(name: &str, want: f64, first: f64) -> f64 {
    let logarithm = 1.0 + want.abs().ln().abs();
    let shape = first.abs().clamp(1e-3, 1.0);
    match name {
        "beta" | "binom" | "comb" => 8.0 + 2.0 * want.abs().ln().abs(),
        "yn" => 32.0,
        "ellipj_sn" | "ellipj_cn" | "ellipj_dn" | "ellipj_ph" => 128.0,
        "lpmv" => 64.0,
        "eval_legendre" | "eval_chebyt" | "eval_chebyu" | "eval_hermite" | "eval_hermitenorm"
        | "eval_laguerre" | "eval_genlaguerre" | "eval_jacobi" | "eval_gegenbauer"
        | "sph_harm_y_re" | "sph_harm_y_im" => 8.0 * (first.abs() + 1.0),
        "j0" | "j1" | "jn" | "y0" | "y1" | "i0" | "i1" | "i0e" | "i1e" | "k0" | "k1" | "k0e"
        | "k1e" | "kn" | "exp1" | "expi" | "expn" | "si" | "ci" | "shi" | "chi" | "fresnel_s"
        | "fresnel_c" | "zeta" | "zetac" | "hurwitz" | "spence" => 16.0,
        _ if name.starts_with("mathieu_mod") => 32.0,
        "pro_cv" | "obl_cv" | "pro_cv_seq" | "obl_cv_seq" => 64.0,
        _ if name.starts_with("pro_ang1") || name.starts_with("obl_ang1") => 16.0,
        _ if name.starts_with("pro_rad") || name.starts_with("obl_rad") => 128.0,
        "gammainc" | "gammaincc" | "gammaincinv" | "gammainccinv" => 16.0 * logarithm / shape,
        "betainc" | "betaincc" | "betaincinv" | "betainccinv" => 32.0 * logarithm / shape,
        "bdtr" | "bdtrc" | "bdtri" | "nbdtr" | "nbdtrc" | "nbdtri" | "pdtr" | "pdtrc" | "pdtri"
        | "chdtr" | "chdtrc" | "chdtri" | "fdtr" | "fdtrc" | "fdtri" | "gdtr" | "gdtrc"
        | "stdtr" | "stdtrit" | "kolmogorov" | "kolmogi" | "smirnov" | "smirnovi" | "owens_t" => {
            32.0 * logarithm
        }
        "jv" | "yv" | "jve" | "yve" | "jvp" | "yvp" | "spherical_jn" | "spherical_yn" | "iv"
        | "ive" | "kv" | "kve" | "ivp" | "kvp" | "spherical_in" | "spherical_kn" => 256.0,
        _ if name.starts_with("airy") => 64.0,
        "ber" | "bei" | "ker" | "kei" | "berp" | "beip" | "kerp" | "keip" | "pbdv" | "pbdv_d"
        | "pbvv" | "pbvv_d" | "pbwa" | "pbwa_d" => 32.0,
        "struve" | "modstruve" | "hyp2f1" => 128.0,
        _ if name.starts_with("mathieu_") || name == "hyp0f1" => 16.0,
        "hyp1f1" => 64.0,
        "hyperu" => 512.0,
        _ if name.starts_with("roots_") && name.ends_with("_w") => 32.0 * first,
        _ if name.starts_with("roots_") => 16.0,
        _ => 8.0,
    }
}

/// The magnitude a function's error is measured against. `gammaln`, `psi`, `betaln`, `expi` and
/// `chi` (the second value of `shichi`) have zeros where no formula keeps relative precision
/// (`gammaln` between −3 and −2, `psi` between each pair of negative whole numbers, `expi` at
/// 0.3725 and `chi` at 0.5238), so they are measured against the larger of the reference and 1, the
/// precision of the terms they are formed from. `ci`, the second value of `sici`, oscillates like
/// sin(x)/x, and is measured against the larger of the reference and 1/x. The Bessel functions of
/// the first and second kind oscillate, so near each zero they are measured against the larger of
/// the reference and their envelope √(2/(π|x|)), for `|x|` above 1. Each part of the complex
/// `lambertw` is measured against the size of the whole value, |W|, which its case records in
/// the place of `x`: a part far smaller than the other has only the precision of the larger.
/// Jacobi's sn, cn and dn are at most 1 in size and come from sines and cosines of the amplitude,
/// so they too are measured against the larger of the reference and 1. An orthogonal polynomial,
/// `lpmv` and a part of `sph_harm_y` are measured against the largest size their recurrence
/// passes through on the way to the degree, which the case records in the place of `x`: near
/// one of the polynomial's zeros the value is a small difference of large terms. A node or
/// weight of Gauss quadrature is measured against the size its case records in the place of `x`:
/// a weight against itself, and a node against the larger of itself and the gap to its nearest
/// neighbour, since Newton's method fixes a node near 0 only to a fraction of that gap.
/// The Bessel functions of real order and the spherical Bessel functions, and the Airy, Kelvin,
/// Struve and hypergeometric functions, are measured against the size their case records in the
/// place of `x`, as the tolerances above describe. So are the Mathieu functions, whose size is
/// the largest of the value, |q ∂f/∂q|, |x ∂f/∂x| and the sum of the sizes of their Fourier
/// terms, which cancel where the function is exponentially small, and the spheroidal functions,
/// whose size is the same with c in the place of q and Legendre terms in the place of Fourier
/// terms; their characteristic values are measured against the larger of |λ| and |c ∂λ/∂c|.
/// A radial function's size adds the modulus √(R1² + R2²) of the two kinds, the envelope they
/// oscillate under, its Bessel terms in the place of Legendre terms, and |R| times the
/// cancellation of the normalizing sum Σ w_r, which costs prolate functions digits as c grows.
pub(super) fn scale(name: &str, want: f64, x: f64) -> f64 {
    match name {
        "gammaln" | "psi" | "betaln" | "expi" | "chi" | "ellipj_sn" | "ellipj_cn" | "ellipj_dn" => {
            want.abs().max(1.0)
        }
        "ci" => want.abs().max(1.0 / x.abs().max(1.0)),
        "lambertw_re" | "lambertw_im" | "eval_legendre" | "eval_chebyt" | "eval_chebyu"
        | "eval_hermite" | "eval_hermitenorm" | "eval_laguerre" | "eval_genlaguerre"
        | "eval_jacobi" | "eval_gegenbauer" | "lpmv" | "sph_harm_y_re" | "sph_harm_y_im" | "jv"
        | "yv" | "iv" | "kv" | "jve" | "yve" | "ive" | "kve" | "jvp" | "yvp" | "ivp" | "kvp"
        | "spherical_jn" | "spherical_yn" | "spherical_in" | "spherical_kn" => x,
        "j0" | "j1" | "jn" | "y0" | "y1" | "yn" if x.abs() > 1.0 => want
            .abs()
            .max((2.0 / (std::f64::consts::PI * x.abs())).sqrt()),
        _ if name.starts_with("roots_")
            || name.starts_with("airy")
            || name.starts_with("mathieu_")
            || name.starts_with("pro_")
            || name.starts_with("obl_") =>
        {
            x
        }
        "ber" | "bei" | "ker" | "kei" | "berp" | "beip" | "kerp" | "keip" | "struve"
        | "modstruve" | "hyp0f1" | "hyp1f1" | "hyperu" | "hyp2f1" | "pbdv" | "pbdv_d" | "pbvv"
        | "pbvv_d" | "pbwa" | "pbwa_d" => x,
        _ => want.abs(),
    }
}
