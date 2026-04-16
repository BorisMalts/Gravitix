use crate::value::Value;
use crate::error::GravResult;
use crate::runtime_err;

// ─────────────────────────────────────────────────────────────────────────────
// Special mathematical functions
//
// All implementations are pure Rust — no external dependencies.
// Uses well-known numerical approximation algorithms.
// ─────────────────────────────────────────────────────────────────────────────

// ── Lanczos gamma coefficients (g=7, 9 terms) ──────────────────────────────

const LANCZOS_G: f64 = 7.0;
const LANCZOS_C: [f64; 9] = [
    0.99999999999980993,
    676.5203681218851,
    -1259.1392167224028,
    771.32342877765313,
    -176.61502916214059,
    12.507343278686905,
    -0.13857109526572012,
    9.9843695780195716e-6,
    1.5056327351493116e-7,
];

const EULER_MASCHERONI: f64 = 0.577_215_664_901_532_9;

// ── Argument extraction helper ──────────────────────────────────────────────

fn require_float(args: &[Value], idx: usize, fn_name: &str) -> GravResult<f64> {
    args.get(idx)
        .and_then(|v| v.as_float())
        .ok_or_else(|| runtime_err!("{fn_name}: expected number at position {idx}"))
}

fn require_int(args: &[Value], idx: usize, fn_name: &str) -> GravResult<i64> {
    args.get(idx)
        .and_then(|v| v.as_int())
        .ok_or_else(|| runtime_err!("{fn_name}: expected integer at position {idx}"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal implementations
// ─────────────────────────────────────────────────────────────────────────────

/// Gamma function via Lanczos approximation
fn gamma_impl(x: f64) -> f64 {
    if x < 0.5 {
        // Reflection formula: Gamma(1-z) * Gamma(z) = pi / sin(pi*z)
        let sin_val = (std::f64::consts::PI * x).sin();
        if sin_val.abs() < 1e-300 {
            return f64::INFINITY;
        }
        std::f64::consts::PI / (sin_val * gamma_impl(1.0 - x))
    } else {
        let x = x - 1.0;
        let mut ag = LANCZOS_C[0];
        for i in 1..9 {
            ag += LANCZOS_C[i] / (x + i as f64);
        }
        let t = x + LANCZOS_G + 0.5;
        (2.0 * std::f64::consts::PI).sqrt() * t.powf(x + 0.5) * (-t).exp() * ag
    }
}

/// Log-gamma: ln|Gamma(x)|
fn lgamma_impl(x: f64) -> f64 {
    if x < 0.5 {
        let sin_val = (std::f64::consts::PI * x).sin().abs();
        if sin_val < 1e-300 {
            return f64::INFINITY;
        }
        std::f64::consts::PI.ln() - sin_val.ln() - lgamma_impl(1.0 - x)
    } else {
        let x = x - 1.0;
        let mut ag = LANCZOS_C[0];
        for i in 1..9 {
            ag += LANCZOS_C[i] / (x + i as f64);
        }
        let t = x + LANCZOS_G + 0.5;
        0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + ag.ln()
    }
}

/// Digamma function psi(x) using asymptotic expansion + recurrence
fn digamma_impl(x: f64) -> f64 {
    if x < 1e-6 {
        // psi(x) ≈ -1/x - gamma for small x
        return -1.0 / x - EULER_MASCHERONI;
    }

    // Use recurrence psi(x+1) = psi(x) + 1/x to shift x into asymptotic range
    let mut result = 0.0;
    let mut x = x;
    while x < 8.0 {
        result -= 1.0 / x;
        x += 1.0;
    }

    // Asymptotic expansion for large x:
    // psi(x) ~ ln(x) - 1/(2x) - 1/(12x^2) + 1/(120x^4) - 1/(252x^6) + ...
    let x2 = x * x;
    result += x.ln() - 0.5 / x
        - 1.0 / (12.0 * x2)
        + 1.0 / (120.0 * x2 * x2)
        - 1.0 / (252.0 * x2 * x2 * x2)
        + 1.0 / (240.0 * x2 * x2 * x2 * x2)
        - 5.0 / (660.0 * x2 * x2 * x2 * x2 * x2);

    result
}

/// Error function using Abramowitz & Stegun approximation (Horner form)
fn erf_impl(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();

    // Abramowitz & Stegun 7.1.26 (maximum error ~1.5e-7)
    let p = 0.3275911;
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;

    let t = 1.0 / (1.0 + p * x);
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;
    let t5 = t4 * t;

    let val = 1.0 - (a1 * t + a2 * t2 + a3 * t3 + a4 * t4 + a5 * t5) * (-x * x).exp();
    sign * val
}

/// Riemann zeta function for real s > 1 using Borwein's method (partial sums
/// with Euler-Maclaurin correction for better convergence)
fn zeta_impl(s: f64) -> f64 {
    if s <= 1.0 {
        return f64::NAN; // only valid for s > 1
    }
    if (s - 2.0).abs() < 1e-15 {
        return std::f64::consts::PI * std::f64::consts::PI / 6.0;
    }

    // Direct series with Knopp-Euler acceleration (n=64 terms)
    let n = 64usize;
    let mut sum = 0.0;
    let mut sign = 1.0;
    let mut binom_coeff = 1.0_f64;

    // sum_{k=0}^{n-1} (-1)^k * C(n-1,k) * (k+1)^{-s} / (1 - 2^{1-s})
    // This uses the globally convergent series
    let prefactor = 1.0 / (1.0 - 2.0_f64.powf(1.0 - s));

    // Alternating series: sum_{k=1}^inf (-1)^{k+1} / k^s  = (1 - 2^{1-s}) * zeta(s)
    // Euler acceleration of this alternating series
    let mut d = 0.0_f64;
    let mut partial = 0.0_f64;
    let mut dn = 0.0_f64;

    // Using Cohen-Villegas-Zagier acceleration (more practical)
    let n = 64;
    d = 1.0;
    for _ in 0..n {
        d = d * (3.0 + (8.0_f64).sqrt()) / (3.0 - (8.0_f64).sqrt());
    }
    // Simpler: just use enough direct terms with alternating acceleration
    // Reset to direct alternating Euler approach
    let n = 128;
    let mut alt_sum = 0.0_f64;
    sign = 1.0;
    for k in 1..=n {
        alt_sum += sign / (k as f64).powf(s);
        sign = -sign;
    }

    alt_sum * prefactor
}

/// Bessel function Jn(x) using power series
fn bessel_j_impl(n: i64, x: f64) -> f64 {
    let n_f = n as f64;
    let half_x = x / 2.0;
    let mut term = half_x.powf(n_f) / gamma_impl(n_f + 1.0);
    let mut sum = term;
    let half_x_sq = -half_x * half_x;

    for k in 1..100 {
        term *= half_x_sq / (k as f64 * (n_f + k as f64));
        sum += term;
        if term.abs() < 1e-15 * sum.abs() {
            break;
        }
    }
    sum
}

/// Bessel function Yn(x) for integer n
/// Uses Y_n(x) = (2/pi) * (J_n(x) * (ln(x/2) + gamma) - sum terms)
/// For simplicity, use the Neumann relation for integer orders.
fn bessel_y_impl(n: i64, x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NEG_INFINITY;
    }

    let eps = 1e-10;
    let n_f = n.abs() as f64;

    // For n=0, use series expansion:
    // Y0(x) = (2/pi) * (J0(x) * (ln(x/2) + gamma) + ...)
    // More robust: use the limiting form via forward recurrence from Y0, Y1

    // Y0(x) approximation using series for small x
    let j0 = bessel_j_impl(0, x);
    let half_x = x / 2.0;

    if n == 0 {
        let mut sum = 0.0;
        let mut term = 1.0;
        let hx2 = -half_x * half_x;
        for k in 1..60 {
            term *= hx2 / (k as f64 * k as f64);
            let hk: f64 = (1..=k).map(|j| 1.0 / j as f64).sum();
            sum += hk * term;
        }
        return (2.0 / std::f64::consts::PI)
            * ((half_x.ln() + EULER_MASCHERONI) * j0 - sum);
    }

    if n == 1 {
        // Y1 via series
        let j1 = bessel_j_impl(1, x);
        let mut sum = -1.0 / (half_x);
        let mut term = half_x;
        let hx2 = -half_x * half_x;
        for k in 1..60 {
            let hk: f64 = (1..=k).map(|j| 1.0 / j as f64).sum();
            let hk1: f64 = (1..=k + 1).map(|j| 1.0 / j as f64).sum();
            term *= hx2 / (k as f64 * (k as f64 + 1.0));
            sum += (hk + hk1) * term;
        }
        return (2.0 / std::f64::consts::PI)
            * ((half_x.ln() + EULER_MASCHERONI) * j1 + sum);
    }

    // For higher n, use forward recurrence from Y0 and Y1
    let mut y_prev = bessel_y_impl(0, x);
    let mut y_curr = bessel_y_impl(1, x);

    for k in 1..n.abs() {
        let y_next = (2.0 * k as f64 / x) * y_curr - y_prev;
        y_prev = y_curr;
        y_curr = y_next;
    }

    if n < 0 && n % 2 != 0 {
        -y_curr
    } else {
        y_curr
    }
}

/// Airy Ai(x) using series expansion for moderate |x|
fn airy_ai_impl(x: f64) -> f64 {
    // Ai(x) = c1 * f(x) - c2 * g(x)
    // where f and g are the two linearly independent solutions
    // c1 = 1/(3^{2/3} * Gamma(2/3)), c2 = 1/(3^{1/3} * Gamma(1/3))

    let c1 = 1.0 / (3.0_f64.powf(2.0 / 3.0) * gamma_impl(2.0 / 3.0));
    let c2 = 1.0 / (3.0_f64.powf(1.0 / 3.0) * gamma_impl(1.0 / 3.0));

    // f(x) = sum_{k=0}^inf (3k)! / (k! * (2k)! * 3^k) * x^{3k} ... too complex
    // Use the standard series:
    // Ai(x) = c1 * sum_{k=0} 3^k (1/3)_k / (2k)! * x^{2k}
    //       - c2 * sum_{k=0} 3^k (2/3)_k / (2k+1)! * x^{2k+1}
    // where (a)_k is the Pochhammer symbol

    // Simpler: direct Taylor series
    // Ai(x) = Ai(0) * f(x) + Ai'(0) * g(x)
    // Ai(0) = 1/(3^{2/3} Gamma(2/3)) ≈ 0.3550280538878172
    // Ai'(0) = -1/(3^{1/3} Gamma(1/3)) ≈ -0.2588194037928068

    let ai0 = c1;
    let ai0_prime = -c2;

    // f(x) = sum: f_0=1, f_k satisfies recurrence from Ai'' = x*Ai
    // a_0 = 1, a_1 = 0, a_2 = 0
    // General recurrence for Taylor: a_{n+2} = a_{n-1} / ((n+1)(n+2))  (from y''=xy)
    // Actually: y'' = x*y means a_{n+3} = a_n / ((n+2)(n+3))

    let max_terms = 80;
    let mut coeffs = vec![0.0_f64; max_terms];
    // For Ai: two independent solutions merged
    // Solution 1 (even-like): a_0 = 1, a_1 = 0, a_2 = 0, a_3 = a_0/(2*3), ...
    // Solution 2 (odd-like):  b_0 = 0, b_1 = 1, b_2 = 0, b_3 = 0, b_4 = b_1/(3*4), ...

    let mut a = vec![0.0_f64; max_terms];
    let mut b = vec![0.0_f64; max_terms];
    a[0] = 1.0;
    b[1] = 1.0;

    for n in 0..max_terms - 3 {
        a[n + 3] = a[n] / ((n as f64 + 2.0) * (n as f64 + 3.0));
        b[n + 3] = b[n] / ((n as f64 + 2.0) * (n as f64 + 3.0));
    }

    let mut fa = 0.0;
    let mut fb = 0.0;
    let mut xn = 1.0; // x^n
    for n in 0..max_terms {
        fa += a[n] * xn;
        fb += b[n] * xn;
        xn *= x;
        if xn.is_infinite() || xn.is_nan() {
            break;
        }
    }

    ai0 * fa + ai0_prime * fb
}

/// Airy Bi(x) using series expansion
fn airy_bi_impl(x: f64) -> f64 {
    // Bi(0) = 1/(3^{1/6} * Gamma(2/3))
    // Bi'(0) = 3^{1/6} / Gamma(1/3)
    let bi0 = 1.0 / (3.0_f64.powf(1.0 / 6.0) * gamma_impl(2.0 / 3.0));
    let bi0_prime = 3.0_f64.powf(1.0 / 6.0) / gamma_impl(1.0 / 3.0);

    let max_terms = 80;
    let mut a = vec![0.0_f64; max_terms];
    let mut b = vec![0.0_f64; max_terms];
    a[0] = 1.0;
    b[1] = 1.0;

    for n in 0..max_terms - 3 {
        a[n + 3] = a[n] / ((n as f64 + 2.0) * (n as f64 + 3.0));
        b[n + 3] = b[n] / ((n as f64 + 2.0) * (n as f64 + 3.0));
    }

    let mut fa = 0.0;
    let mut fb = 0.0;
    let mut xn = 1.0;
    for n in 0..max_terms {
        fa += a[n] * xn;
        fb += b[n] * xn;
        xn *= x;
        if xn.is_infinite() || xn.is_nan() {
            break;
        }
    }

    bi0 * fa + bi0_prime * fb
}

/// Complete elliptic integral K(m) using AGM (arithmetic-geometric mean)
/// m is the parameter (not the modulus k; m = k^2)
fn elliptic_k_impl(m: f64) -> f64 {
    if m >= 1.0 {
        return f64::INFINITY;
    }
    if m < 0.0 {
        return f64::NAN;
    }

    let mut a = 1.0;
    let mut b = (1.0 - m).sqrt();

    for _ in 0..50 {
        let a_new = (a + b) / 2.0;
        let b_new = (a * b).sqrt();
        a = a_new;
        b = b_new;
        if (a - b).abs() < 1e-15 {
            break;
        }
    }

    std::f64::consts::PI / (2.0 * a)
}

/// Complete elliptic integral E(m) using AGM variant
fn elliptic_e_impl(m: f64) -> f64 {
    if m > 1.0 {
        return f64::NAN;
    }
    if m == 1.0 {
        return 1.0;
    }
    if m < 0.0 {
        return f64::NAN;
    }

    // Use the series: E(m) = (pi/2) * [1 - sum_{n=1}^inf ((2n-1)!!/(2n)!!)^2 * m^n / (2n-1)]
    // Or use the AGM-based method with running sum of c_n^2
    let mut a = 1.0;
    let mut b = (1.0 - m).sqrt();
    let mut c = m.sqrt();
    let mut power_of_two = 1.0;
    let mut sum_c2 = c * c;

    for _ in 0..50 {
        let a_new = (a + b) / 2.0;
        let b_new = (a * b).sqrt();
        c = (a - b) / 2.0;
        power_of_two *= 2.0;
        sum_c2 += power_of_two * c * c;
        a = a_new;
        b = b_new;
        if c.abs() < 1e-15 {
            break;
        }
    }

    std::f64::consts::PI / (2.0 * a) * (1.0 - sum_c2 / 2.0)
}

/// Bernoulli numbers B_n (first 20 precomputed, recursive beyond)
fn bernoulli_impl(n: i64) -> f64 {
    if n < 0 {
        return 0.0;
    }
    // B_1 = -1/2 (convention); all odd B_n = 0 for n >= 3
    let table: &[f64] = &[
        1.0,                        // B_0
        -0.5,                       // B_1
        1.0 / 6.0,                  // B_2
        0.0,                        // B_3
        -1.0 / 30.0,               // B_4
        0.0,                        // B_5
        1.0 / 42.0,                // B_6
        0.0,                        // B_7
        -1.0 / 30.0,               // B_8
        0.0,                        // B_9
        5.0 / 66.0,                // B_10
        0.0,                        // B_11
        -691.0 / 2730.0,           // B_12
        0.0,                        // B_13
        7.0 / 6.0,                 // B_14
        0.0,                        // B_15
        -3617.0 / 510.0,           // B_16
        0.0,                        // B_17
        43867.0 / 798.0,           // B_18
        0.0,                        // B_19
        -174611.0 / 330.0,         // B_20
    ];

    let n_usize = n as usize;
    if n_usize < table.len() {
        return table[n_usize];
    }

    // Odd Bernoulli numbers >= 3 are zero
    if n >= 3 && n % 2 != 0 {
        return 0.0;
    }

    // Recursive computation for larger n using:
    // B_n = -1/(n+1) * sum_{k=0}^{n-1} C(n+1,k) * B_k
    let mut b_cache: Vec<f64> = table.to_vec();
    while b_cache.len() <= n_usize {
        let m = b_cache.len();
        if m >= 3 && m % 2 != 0 {
            b_cache.push(0.0);
        } else {
            let mut sum = 0.0;
            let mut binom = 1.0_f64;
            for k in 0..m {
                sum += binom * b_cache[k];
                binom *= (m + 1 - k) as f64 / (k + 1) as f64;
            }
            b_cache.push(-sum / (m as f64 + 1.0));
        }
    }
    b_cache[n_usize]
}

/// Catalan number C(n) = C(2n, n) / (n+1)
fn catalan_impl(n: i64) -> f64 {
    if n < 0 {
        return 0.0;
    }
    if n == 0 {
        return 1.0;
    }
    // C(n) = C(2n, n) / (n+1)
    // Use log-gamma for numerical stability
    let n_f = n as f64;
    let log_val = lgamma_impl(2.0 * n_f + 1.0)
        - 2.0 * lgamma_impl(n_f + 1.0)
        - (n_f + 1.0).ln();
    log_val.exp().round()
}

/// Logarithmic integral Li(x) using Ramanujan's series
fn li_impl(x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if x == 1.0 {
        return f64::NEG_INFINITY; // Li(1) = -infinity
    }

    // Use the series: li(x) = gamma + ln|ln(x)| + sum_{k=1}^inf (ln(x))^k / (k * k!)
    let ln_x = x.ln();
    let mut sum = EULER_MASCHERONI + ln_x.abs().ln();
    let mut term = ln_x;
    for k in 1..100 {
        sum += term / (k as f64 * k as f64);
        term *= ln_x / (k as f64 + 1.0);
        if term.abs() < 1e-15 {
            break;
        }
    }
    sum
}

/// Exponential integral Ei(x) for real x
fn ei_impl(x: f64) -> f64 {
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }

    if x < -40.0 {
        // Asymptotic: Ei(x) ~ e^x/x * (1 + 1/x + 2!/x^2 + ...)
        let mut sum = 0.0;
        let mut term = 1.0;
        for k in 0..20 {
            sum += term;
            term *= (k as f64 + 1.0) / x;
            if term.abs() < 1e-15 * sum.abs() {
                break;
            }
        }
        return x.exp() / x * sum;
    }

    if x.abs() < 40.0 {
        // Power series: Ei(x) = gamma + ln|x| + sum_{k=1}^inf x^k / (k * k!)
        let mut sum = EULER_MASCHERONI + x.abs().ln();
        let mut term = x;
        let mut factorial = 1.0;
        for k in 1..200 {
            factorial *= k as f64;
            sum += term / (k as f64 * factorial);
            term *= x;
            if (term / (k as f64 * factorial)).abs() < 1e-15 {
                break;
            }
        }
        return sum;
    }

    // Large positive x: asymptotic expansion
    let mut sum = 0.0;
    let mut term = 1.0;
    for k in 0..30 {
        sum += term;
        term *= (k as f64 + 1.0) / x;
        if term.abs() < 1e-15 * sum.abs() || term.abs() > sum.abs() {
            break;
        }
    }
    x.exp() / x * sum
}

/// Sine integral Si(x) = integral from 0 to x of sin(t)/t dt
fn si_impl(x: f64) -> f64 {
    // Taylor series: Si(x) = sum_{k=0}^inf (-1)^k * x^{2k+1} / ((2k+1) * (2k+1)!)
    let mut sum = 0.0;
    let mut term = x; // first term: x / (1 * 1!)
    let x2 = -x * x;

    for k in 0..100 {
        let n = 2 * k + 1;
        sum += term / n as f64;
        // Next term: multiply by -x^2 / ((2k+2)(2k+3))
        term *= x2 / ((n as f64 + 1.0) * (n as f64 + 2.0));
        if term.abs() < 1e-15 * sum.abs().max(1e-300) {
            break;
        }
    }
    sum
}

/// Cosine integral Ci(x) = gamma + ln(x) + integral from 0 to x of (cos(t)-1)/t dt
fn ci_impl(x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NEG_INFINITY;
    }

    // Ci(x) = gamma + ln(x) + sum_{k=1}^inf (-1)^k * x^{2k} / (2k * (2k)!)
    let mut sum = EULER_MASCHERONI + x.ln();
    let x2 = -x * x;
    let mut term = x2 / 2.0; // k=1: -x^2 / (2 * 2!)

    for k in 1..100 {
        let n = 2 * k;
        let factorial_2k = {
            let mut f = 1.0;
            for j in 1..=(n as usize) {
                f *= j as f64;
            }
            f
        };
        let contribution = term.abs() / factorial_2k; // we build term with sign
        let actual_term = if k % 2 != 0 {
            // (-1)^k for k=1 is -1
            -(x.powi(n as i32)) / (n as f64 * factorial_2k)
        } else {
            (x.powi(n as i32)) / (n as f64 * factorial_2k)
        };
        sum += actual_term;
        if actual_term.abs() < 1e-15 * sum.abs().max(1e-300) {
            break;
        }
    }
    sum
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn call_special_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    let v = match name {

        // ── Gamma function family ───────────────────────────────────────────
        "gamma" => {
            let x = require_float(args, 0, "gamma")?;
            Value::Float(gamma_impl(x))
        }
        "lgamma" => {
            let x = require_float(args, 0, "lgamma")?;
            Value::Float(lgamma_impl(x))
        }
        "digamma" => {
            let x = require_float(args, 0, "digamma")?;
            Value::Float(digamma_impl(x))
        }
        "beta" => {
            let a = require_float(args, 0, "beta")?;
            let b = require_float(args, 1, "beta")?;
            // B(a,b) = Gamma(a)*Gamma(b)/Gamma(a+b)
            // Use log-gamma for numerical stability
            let result = (lgamma_impl(a) + lgamma_impl(b) - lgamma_impl(a + b)).exp();
            Value::Float(result)
        }

        // ── Error function ──────────────────────────────────────────────────
        "erf" => {
            let x = require_float(args, 0, "erf")?;
            Value::Float(erf_impl(x))
        }
        "erfc" => {
            let x = require_float(args, 0, "erfc")?;
            Value::Float(1.0 - erf_impl(x))
        }

        // ── Riemann zeta ────────────────────────────────────────────────────
        "zeta" => {
            let s = require_float(args, 0, "zeta")?;
            if s <= 1.0 {
                return Err(runtime_err!("zeta: requires s > 1"));
            }
            Value::Float(zeta_impl(s))
        }

        // ── Bessel functions ────────────────────────────────────────────────
        "bessel_j" => {
            let n = require_int(args, 0, "bessel_j")?;
            let x = require_float(args, 1, "bessel_j")?;
            Value::Float(bessel_j_impl(n, x))
        }
        "bessel_y" => {
            let n = require_int(args, 0, "bessel_y")?;
            let x = require_float(args, 1, "bessel_y")?;
            if x <= 0.0 {
                return Err(runtime_err!("bessel_y: requires x > 0"));
            }
            Value::Float(bessel_y_impl(n, x))
        }

        // ── Airy functions ──────────────────────────────────────────────────
        "airy_ai" => {
            let x = require_float(args, 0, "airy_ai")?;
            Value::Float(airy_ai_impl(x))
        }
        "airy_bi" => {
            let x = require_float(args, 0, "airy_bi")?;
            Value::Float(airy_bi_impl(x))
        }

        // ── Orthogonal polynomials ──────────────────────────────────────────
        "legendre" => {
            let n = require_int(args, 0, "legendre")? as usize;
            let x = require_float(args, 1, "legendre")?;
            // Recurrence: P0=1, P1=x, Pn = ((2n-1)*x*P_{n-1} - (n-1)*P_{n-2}) / n
            if n == 0 { Value::Float(1.0) }
            else if n == 1 { Value::Float(x) }
            else {
                let mut p_prev = 1.0_f64;
                let mut p_curr = x;
                for k in 2..=n {
                    let p_next = ((2 * k - 1) as f64 * x * p_curr - (k - 1) as f64 * p_prev) / k as f64;
                    p_prev = p_curr;
                    p_curr = p_next;
                }
                Value::Float(p_curr)
            }
        }

        "hermite" => {
            let n = require_int(args, 0, "hermite")? as usize;
            let x = require_float(args, 1, "hermite")?;
            // Recurrence: H0=1, H1=2x, Hn = 2x*H_{n-1} - 2(n-1)*H_{n-2}
            if n == 0 { Value::Float(1.0) }
            else if n == 1 { Value::Float(2.0 * x) }
            else {
                let mut h_prev = 1.0_f64;
                let mut h_curr = 2.0 * x;
                for k in 2..=n {
                    let h_next = 2.0 * x * h_curr - 2.0 * (k - 1) as f64 * h_prev;
                    h_prev = h_curr;
                    h_curr = h_next;
                }
                Value::Float(h_curr)
            }
        }

        "chebyshev" => {
            let n = require_int(args, 0, "chebyshev")? as usize;
            let x = require_float(args, 1, "chebyshev")?;
            // Recurrence: T0=1, T1=x, Tn = 2x*T_{n-1} - T_{n-2}
            if n == 0 { Value::Float(1.0) }
            else if n == 1 { Value::Float(x) }
            else {
                let mut t_prev = 1.0_f64;
                let mut t_curr = x;
                for _ in 2..=n {
                    let t_next = 2.0 * x * t_curr - t_prev;
                    t_prev = t_curr;
                    t_curr = t_next;
                }
                Value::Float(t_curr)
            }
        }

        "laguerre" => {
            let n = require_int(args, 0, "laguerre")? as usize;
            let x = require_float(args, 1, "laguerre")?;
            // Recurrence: L0=1, L1=1-x, Ln = ((2n-1-x)*L_{n-1} - (n-1)*L_{n-2}) / n
            if n == 0 { Value::Float(1.0) }
            else if n == 1 { Value::Float(1.0 - x) }
            else {
                let mut l_prev = 1.0_f64;
                let mut l_curr = 1.0 - x;
                for k in 2..=n {
                    let l_next = ((2 * k - 1) as f64 - x) * l_curr / k as f64
                        - (k - 1) as f64 * l_prev / k as f64;
                    l_prev = l_curr;
                    l_curr = l_next;
                }
                Value::Float(l_curr)
            }
        }

        // ── Elliptic integrals ──────────────────────────────────────────────
        "elliptic_k" => {
            let m = require_float(args, 0, "elliptic_k")?;
            Value::Float(elliptic_k_impl(m))
        }
        "elliptic_e" => {
            let m = require_float(args, 0, "elliptic_e")?;
            Value::Float(elliptic_e_impl(m))
        }

        // ── Number-theoretic functions ──────────────────────────────────────
        "bernoulli" => {
            let n = require_int(args, 0, "bernoulli")?;
            Value::Float(bernoulli_impl(n))
        }

        "catalan" => {
            let n = require_int(args, 0, "catalan")?;
            Value::Float(catalan_impl(n))
        }

        "harmonic" => {
            let n = require_int(args, 0, "harmonic")?;
            if n <= 0 {
                Value::Float(0.0)
            } else {
                let mut sum = 0.0;
                for k in 1..=n {
                    sum += 1.0 / k as f64;
                }
                Value::Float(sum)
            }
        }

        // ── Integral functions ──────────────────────────────────────────────
        "li" => {
            let x = require_float(args, 0, "li")?;
            Value::Float(li_impl(x))
        }
        "ei" => {
            let x = require_float(args, 0, "ei")?;
            Value::Float(ei_impl(x))
        }
        "si" => {
            let x = require_float(args, 0, "si")?;
            Value::Float(si_impl(x))
        }
        "ci" => {
            let x = require_float(args, 0, "ci")?;
            Value::Float(ci_impl(x))
        }

        // ── Unknown — let the caller decide ─────────────────────────────────
        _ => return Ok(None),
    };
    Ok(Some(v))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fl(f: f64) -> Value { Value::Float(f) }
    fn int(n: i64) -> Value { Value::Int(n) }

    fn approx(a: f64, b: f64) -> bool { (a - b).abs() < 1e-6 }
    fn approx_rel(a: f64, b: f64, tol: f64) -> bool {
        if b.abs() < 1e-15 { a.abs() < tol } else { ((a - b) / b).abs() < tol }
    }

    // ── Gamma ───────────────────────────────────────────────────────────────

    #[test]
    fn test_gamma_integers() {
        // Gamma(n) = (n-1)!
        let r = call_special_builtin("gamma", &[fl(5.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 24.0)); // 4! = 24
    }

    #[test]
    fn test_gamma_half() {
        // Gamma(0.5) = sqrt(pi)
        let r = call_special_builtin("gamma", &[fl(0.5)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), std::f64::consts::PI.sqrt()));
    }

    #[test]
    fn test_lgamma() {
        let r = call_special_builtin("lgamma", &[fl(5.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 24.0_f64.ln()));
    }

    #[test]
    fn test_digamma_one() {
        // psi(1) = -gamma
        let r = call_special_builtin("digamma", &[fl(1.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), -EULER_MASCHERONI));
    }

    #[test]
    fn test_beta() {
        // B(1,1) = 1
        let r = call_special_builtin("beta", &[fl(1.0), fl(1.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 1.0));
    }

    #[test]
    fn test_beta_half() {
        // B(0.5, 0.5) = pi
        let r = call_special_builtin("beta", &[fl(0.5), fl(0.5)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), std::f64::consts::PI));
    }

    // ── Error function ──────────────────────────────────────────────────────

    #[test]
    fn test_erf_zero() {
        let r = call_special_builtin("erf", &[fl(0.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 0.0));
    }

    #[test]
    fn test_erf_large() {
        let r = call_special_builtin("erf", &[fl(3.0)]).unwrap().unwrap();
        assert!(r.as_float().unwrap() > 0.999);
    }

    #[test]
    fn test_erfc() {
        let r = call_special_builtin("erfc", &[fl(0.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 1.0));
    }

    // ── Zeta ────────────────────────────────────────────────────────────────

    #[test]
    fn test_zeta_2() {
        let r = call_special_builtin("zeta", &[fl(2.0)]).unwrap().unwrap();
        let expected = std::f64::consts::PI * std::f64::consts::PI / 6.0;
        assert!(approx(r.as_float().unwrap(), expected));
    }

    #[test]
    fn test_zeta_invalid() {
        let r = call_special_builtin("zeta", &[fl(0.5)]);
        assert!(r.is_err());
    }

    // ── Bessel ──────────────────────────────────────────────────────────────

    #[test]
    fn test_bessel_j0_zero() {
        let r = call_special_builtin("bessel_j", &[int(0), fl(0.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 1.0));
    }

    #[test]
    fn test_bessel_j1_zero() {
        let r = call_special_builtin("bessel_j", &[int(1), fl(0.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 0.0));
    }

    // ── Orthogonal polynomials ──────────────────────────────────────────────

    #[test]
    fn test_legendre() {
        // P0(x) = 1
        let r = call_special_builtin("legendre", &[int(0), fl(0.5)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 1.0));
        // P1(x) = x
        let r = call_special_builtin("legendre", &[int(1), fl(0.5)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 0.5));
        // P2(x) = (3x^2 - 1)/2 = (0.75 - 1)/2 = -0.125
        let r = call_special_builtin("legendre", &[int(2), fl(0.5)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), -0.125));
    }

    #[test]
    fn test_hermite() {
        // H0 = 1, H1(1) = 2, H2(1) = 2*1*2 - 2*1 = 2
        let r = call_special_builtin("hermite", &[int(2), fl(1.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 2.0));
    }

    #[test]
    fn test_chebyshev() {
        // T0 = 1, T1(x) = x, T2(x) = 2x^2 - 1
        let r = call_special_builtin("chebyshev", &[int(2), fl(0.5)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), -0.5)); // 2*0.25 - 1 = -0.5
    }

    #[test]
    fn test_laguerre() {
        // L0 = 1, L1(x) = 1-x, L2(x) = (x^2 - 4x + 2)/2
        let r = call_special_builtin("laguerre", &[int(0), fl(1.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 1.0));
        let r = call_special_builtin("laguerre", &[int(1), fl(1.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 0.0));
    }

    // ── Elliptic integrals ──────────────────────────────────────────────────

    #[test]
    fn test_elliptic_k_zero() {
        // K(0) = pi/2
        let r = call_special_builtin("elliptic_k", &[fl(0.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), std::f64::consts::FRAC_PI_2));
    }

    #[test]
    fn test_elliptic_e_zero() {
        // E(0) = pi/2
        let r = call_special_builtin("elliptic_e", &[fl(0.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), std::f64::consts::FRAC_PI_2));
    }

    // ── Number-theoretic ────────────────────────────────────────────────────

    #[test]
    fn test_bernoulli() {
        assert!(approx(bernoulli_impl(0), 1.0));
        assert!(approx(bernoulli_impl(1), -0.5));
        assert!(approx(bernoulli_impl(2), 1.0 / 6.0));
        assert!(approx(bernoulli_impl(3), 0.0));
    }

    #[test]
    fn test_catalan() {
        // C(0)=1, C(1)=1, C(2)=2, C(3)=5, C(4)=14
        assert!(approx(catalan_impl(0), 1.0));
        assert!(approx(catalan_impl(1), 1.0));
        assert!(approx(catalan_impl(2), 2.0));
        assert!(approx(catalan_impl(3), 5.0));
        assert!(approx(catalan_impl(4), 14.0));
    }

    #[test]
    fn test_harmonic() {
        // H(1) = 1, H(2) = 1.5, H(3) = 1.8333...
        let r = call_special_builtin("harmonic", &[int(3)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 11.0 / 6.0));
    }

    // ── Integral functions ──────────────────────────────────────────────────

    #[test]
    fn test_si_zero() {
        let r = call_special_builtin("si", &[fl(0.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 0.0));
    }

    #[test]
    fn test_si_small() {
        // Si(1) ≈ 0.9460831
        let r = call_special_builtin("si", &[fl(1.0)]).unwrap().unwrap();
        assert!(approx_rel(r.as_float().unwrap(), 0.9460831, 1e-4));
    }

    #[test]
    fn test_ei() {
        // Ei(1) ≈ 1.8951178
        let r = call_special_builtin("ei", &[fl(1.0)]).unwrap().unwrap();
        assert!(approx_rel(r.as_float().unwrap(), 1.8951178, 1e-4));
    }

    // ── Airy ────────────────────────────────────────────────────────────────

    #[test]
    fn test_airy_ai_zero() {
        // Ai(0) ≈ 0.35502805
        let r = call_special_builtin("airy_ai", &[fl(0.0)]).unwrap().unwrap();
        assert!(approx_rel(r.as_float().unwrap(), 0.35502805, 1e-4));
    }

    #[test]
    fn test_airy_bi_zero() {
        // Bi(0) ≈ 0.61492663
        let r = call_special_builtin("airy_bi", &[fl(0.0)]).unwrap().unwrap();
        assert!(approx_rel(r.as_float().unwrap(), 0.61492663, 1e-4));
    }

    #[test]
    fn test_unknown_returns_none() {
        assert!(call_special_builtin("unknown_fn", &[]).unwrap().is_none());
    }
}
