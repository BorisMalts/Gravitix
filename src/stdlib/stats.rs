use std::collections::HashMap;

use crate::value::Value;
use crate::error::GravResult;
use crate::runtime_err;

// ─────────────────────────────────────────────────────────────────────────────
// Helper — extract a Vec<f64> from a Value::List
// ─────────────────────────────────────────────────────────────────────────────

fn to_f64_list(v: &Value) -> Option<Vec<f64>> {
    if let Value::List(l) = v {
        l.borrow().iter().map(|v| v.as_float()).collect()
    } else {
        None
    }
}

fn get_float(args: &[Value], idx: usize, fn_name: &str) -> GravResult<f64> {
    args.get(idx)
        .and_then(|v| v.as_float())
        .ok_or_else(|| runtime_err!("{fn_name}: expected number at position {idx}"))
}

fn get_int(args: &[Value], idx: usize, fn_name: &str) -> GravResult<i64> {
    args.get(idx)
        .and_then(|v| v.as_int())
        .ok_or_else(|| runtime_err!("{fn_name}: expected integer at position {idx}"))
}

fn require_list(args: &[Value], idx: usize, fn_name: &str) -> GravResult<Vec<f64>> {
    let val = args.get(idx)
        .ok_or_else(|| runtime_err!("{fn_name}: expected list at position {idx}"))?;
    to_f64_list(val)
        .ok_or_else(|| runtime_err!("{fn_name}: argument at position {idx} must be a list of numbers"))
}

fn require_nonempty_list(args: &[Value], idx: usize, fn_name: &str) -> GravResult<Vec<f64>> {
    let list = require_list(args, idx, fn_name)?;
    if list.is_empty() {
        return Err(runtime_err!("{fn_name}: list must not be empty"));
    }
    Ok(list)
}

// ─────────────────────────────────────────────────────────────────────────────
// Error function approximation (Abramowitz & Stegun)
// ─────────────────────────────────────────────────────────────────────────────

fn erf_approx(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let poly = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let result = 1.0 - poly * (-x * x).exp();
    if x >= 0.0 { result } else { -result }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn call_stats_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    let v = match name {
        // ── Descriptive statistics ───────────────────────────────────────────
        "sum" => {
            let list = require_list(args, 0, "sum")?;
            Value::Float(list.iter().sum())
        }

        "avg" | "mean" => {
            let list = require_nonempty_list(args, 0, name)?;
            let s: f64 = list.iter().sum();
            Value::Float(s / list.len() as f64)
        }

        "median" => {
            let mut list = require_nonempty_list(args, 0, "median")?;
            list.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let n = list.len();
            let mid = n / 2;
            if n % 2 == 0 {
                Value::Float((list[mid - 1] + list[mid]) / 2.0)
            } else {
                Value::Float(list[mid])
            }
        }

        "mode" => {
            let list = require_nonempty_list(args, 0, "mode")?;
            // Discretize by rounding to avoid floating-point comparison issues
            // Use a string representation for exact matching
            let mut counts: HashMap<String, (f64, usize)> = HashMap::new();
            for &x in &list {
                let key = format!("{}", x);
                let entry = counts.entry(key).or_insert((x, 0));
                entry.1 += 1;
            }
            let (mode_val, _) = counts.values()
                .max_by_key(|(_, count)| *count)
                .unwrap(); // safe: list is non-empty
            Value::Float(*mode_val)
        }

        "variance" => {
            let list = require_nonempty_list(args, 0, "variance")?;
            let mean: f64 = list.iter().sum::<f64>() / list.len() as f64;
            let var: f64 = list.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / list.len() as f64;
            Value::Float(var)
        }

        "stddev" => {
            let list = require_nonempty_list(args, 0, "stddev")?;
            let mean: f64 = list.iter().sum::<f64>() / list.len() as f64;
            let var: f64 = list.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / list.len() as f64;
            Value::Float(var.sqrt())
        }

        "cov" => {
            let a = require_nonempty_list(args, 0, "cov")?;
            let b = require_nonempty_list(args, 1, "cov")?;
            if a.len() != b.len() {
                return Err(runtime_err!("cov: lists must have the same length ({} vs {})", a.len(), b.len()));
            }
            let n = a.len() as f64;
            let mean_a: f64 = a.iter().sum::<f64>() / n;
            let mean_b: f64 = b.iter().sum::<f64>() / n;
            let cov: f64 = a.iter().zip(b.iter())
                .map(|(x, y)| (x - mean_a) * (y - mean_b))
                .sum::<f64>() / n;
            Value::Float(cov)
        }

        "corr" => {
            let a = require_nonempty_list(args, 0, "corr")?;
            let b = require_nonempty_list(args, 1, "corr")?;
            if a.len() != b.len() {
                return Err(runtime_err!("corr: lists must have the same length ({} vs {})", a.len(), b.len()));
            }
            let n = a.len() as f64;
            let mean_a: f64 = a.iter().sum::<f64>() / n;
            let mean_b: f64 = b.iter().sum::<f64>() / n;
            let cov: f64 = a.iter().zip(b.iter())
                .map(|(x, y)| (x - mean_a) * (y - mean_b))
                .sum::<f64>() / n;
            let std_a: f64 = (a.iter().map(|x| (x - mean_a).powi(2)).sum::<f64>() / n).sqrt();
            let std_b: f64 = (b.iter().map(|x| (x - mean_b).powi(2)).sum::<f64>() / n).sqrt();
            if std_a < 1e-12 || std_b < 1e-12 {
                return Err(runtime_err!("corr: standard deviation is zero, correlation undefined"));
            }
            Value::Float(cov / (std_a * std_b))
        }

        "percentile" => {
            let mut list = require_nonempty_list(args, 0, "percentile")?;
            let p = get_float(args, 1, "percentile")?;
            if !(0.0..=100.0).contains(&p) {
                return Err(runtime_err!("percentile: p must be between 0 and 100, got {}", p));
            }
            list.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let n = list.len();
            if n == 1 {
                Value::Float(list[0])
            } else {
                let rank = (p / 100.0) * (n - 1) as f64;
                let lower = rank.floor() as usize;
                let upper = rank.ceil() as usize;
                let frac = rank - lower as f64;
                if lower == upper {
                    Value::Float(list[lower])
                } else {
                    Value::Float(list[lower] * (1.0 - frac) + list[upper] * frac)
                }
            }
        }

        "zscore" => {
            let list = require_nonempty_list(args, 0, "zscore")?;
            let n = list.len() as f64;
            let mean: f64 = list.iter().sum::<f64>() / n;
            let stddev: f64 = (list.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n).sqrt();
            if stddev < 1e-12 {
                return Err(runtime_err!("zscore: standard deviation is zero"));
            }
            let scores: Vec<Value> = list.iter()
                .map(|x| Value::Float((x - mean) / stddev))
                .collect();
            Value::make_list(scores)
        }

        "min_of" => {
            let list = require_nonempty_list(args, 0, "min_of")?;
            let min = list.iter().cloned().fold(f64::INFINITY, f64::min);
            Value::Float(min)
        }

        "max_of" => {
            let list = require_nonempty_list(args, 0, "max_of")?;
            let max = list.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            Value::Float(max)
        }

        // ── Probability distributions ───────────────────────────────────────
        "normal_pdf" => {
            let x = get_float(args, 0, "normal_pdf")?;
            let mu = get_float(args, 1, "normal_pdf")?;
            let sigma = get_float(args, 2, "normal_pdf")?;
            if sigma <= 0.0 {
                return Err(runtime_err!("normal_pdf: sigma must be positive"));
            }
            let z = (x - mu) / sigma;
            let pdf = (-0.5 * z * z).exp() / (sigma * (2.0 * std::f64::consts::PI).sqrt());
            Value::Float(pdf)
        }

        "normal_cdf" => {
            let x = get_float(args, 0, "normal_cdf")?;
            let mu = get_float(args, 1, "normal_cdf")?;
            let sigma = get_float(args, 2, "normal_cdf")?;
            if sigma <= 0.0 {
                return Err(runtime_err!("normal_cdf: sigma must be positive"));
            }
            let z = (x - mu) / sigma;
            let cdf = 0.5 * (1.0 + erf_approx(z / std::f64::consts::SQRT_2));
            Value::Float(cdf)
        }

        "poisson_pmf" => {
            let k = get_int(args, 0, "poisson_pmf")?;
            let lambda = get_float(args, 1, "poisson_pmf")?;
            if k < 0 {
                return Err(runtime_err!("poisson_pmf: k must be non-negative"));
            }
            if lambda < 0.0 {
                return Err(runtime_err!("poisson_pmf: lambda must be non-negative"));
            }
            // P(X=k) = e^(-λ) * λ^k / k!
            // Compute in log-space to avoid overflow
            let log_pmf = -lambda + k as f64 * lambda.ln() - ln_factorial(k as u64);
            Value::Float(log_pmf.exp())
        }

        "binomial_pmf" => {
            let k = get_int(args, 0, "binomial_pmf")?;
            let n = get_int(args, 1, "binomial_pmf")?;
            let p = get_float(args, 2, "binomial_pmf")?;
            if k < 0 || k > n {
                Value::Float(0.0)
            } else if !(0.0..=1.0).contains(&p) {
                return Err(runtime_err!("binomial_pmf: p must be between 0 and 1"));
            } else {
                // C(n,k) * p^k * (1-p)^(n-k) — compute in log-space
                let log_binom = ln_factorial(n as u64) - ln_factorial(k as u64) - ln_factorial((n - k) as u64);
                let log_pmf = log_binom + k as f64 * p.ln() + (n - k) as f64 * (1.0 - p).ln();
                Value::Float(log_pmf.exp())
            }
        }

        // ── Random number generation ────────────────────────────────────────
        "uniform_rand" => {
            let a = get_float(args, 0, "uniform_rand")?;
            let b = get_float(args, 1, "uniform_rand")?;
            if a >= b {
                return Err(runtime_err!("uniform_rand: a must be less than b"));
            }
            let r = simple_random_f64();
            Value::Float(a + r * (b - a))
        }

        "normal_rand" => {
            let mu = get_float(args, 0, "normal_rand")?;
            let sigma = get_float(args, 1, "normal_rand")?;
            if sigma <= 0.0 {
                return Err(runtime_err!("normal_rand: sigma must be positive"));
            }
            // Box-Muller transform
            let u1 = simple_random_f64().max(1e-15); // avoid log(0)
            let u2 = simple_random_f64();
            let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
            Value::Float(mu + sigma * z)
        }

        // ── Linear regression ───────────────────────────────────────────────
        "linreg" => {
            let xs = require_nonempty_list(args, 0, "linreg")?;
            let ys = require_nonempty_list(args, 1, "linreg")?;
            if xs.len() != ys.len() {
                return Err(runtime_err!("linreg: xs and ys must have the same length ({} vs {})", xs.len(), ys.len()));
            }
            if xs.len() < 2 {
                return Err(runtime_err!("linreg: need at least 2 data points"));
            }
            let n = xs.len() as f64;
            let sum_x: f64 = xs.iter().sum();
            let sum_y: f64 = ys.iter().sum();
            let sum_xy: f64 = xs.iter().zip(ys.iter()).map(|(x, y)| x * y).sum();
            let sum_x2: f64 = xs.iter().map(|x| x * x).sum();

            let denom = n * sum_x2 - sum_x * sum_x;
            if denom.abs() < 1e-12 {
                return Err(runtime_err!("linreg: all x values are the same, slope undefined"));
            }

            let slope = (n * sum_xy - sum_x * sum_y) / denom;
            let intercept = (sum_y - slope * sum_x) / n;

            // R-squared
            let mean_y = sum_y / n;
            let ss_tot: f64 = ys.iter().map(|y| (y - mean_y).powi(2)).sum();
            let ss_res: f64 = xs.iter().zip(ys.iter())
                .map(|(x, y)| {
                    let predicted = slope * x + intercept;
                    (y - predicted).powi(2)
                })
                .sum();
            let r_squared = if ss_tot.abs() < 1e-12 { 1.0 } else { 1.0 - ss_res / ss_tot };

            let mut m = HashMap::new();
            m.insert("slope".to_string(), Value::Float(slope));
            m.insert("intercept".to_string(), Value::Float(intercept));
            m.insert("r_squared".to_string(), Value::Float(r_squared));
            Value::make_map(m)
        }

        // ── Unknown — let the caller decide ─────────────────────────────────
        _ => return Ok(None),
    };
    Ok(Some(v))
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Natural log of n! — computed iteratively.
fn ln_factorial(n: u64) -> f64 {
    let mut result = 0.0;
    for i in 2..=n {
        result += (i as f64).ln();
    }
    result
}

/// Simple pseudo-random float in [0, 1) using system time as entropy.
/// Not cryptographically secure — adequate for scripting language random.
fn simple_random_f64() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(42);
    // xorshift-style mixing for better distribution
    let mut x = nanos.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    (x >> 11) as f64 / (1u64 << 53) as f64
}
