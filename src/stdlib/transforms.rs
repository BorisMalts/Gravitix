use crate::value::Value;
use crate::error::GravResult;
use crate::runtime_err;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers — convert between Value and Rust numeric types
// ─────────────────────────────────────────────────────────────────────────────

fn to_f64_list(v: &Value) -> Option<Vec<f64>> {
    if let Value::List(l) = v {
        l.borrow().iter().map(|v| v.as_float()).collect()
    } else {
        None
    }
}

fn to_complex_list(v: &Value) -> Option<Vec<(f64, f64)>> {
    if let Value::List(l) = v {
        l.borrow().iter().map(|pair| {
            if let Value::List(inner) = pair {
                let inner = inner.borrow();
                if inner.len() == 2 {
                    let re = inner[0].as_float()?;
                    let im = inner[1].as_float()?;
                    Some((re, im))
                } else {
                    None
                }
            } else {
                None
            }
        }).collect()
    } else {
        None
    }
}

fn from_complex_list(data: &[(f64, f64)]) -> Value {
    Value::make_list(
        data.iter()
            .map(|(re, im)| {
                Value::make_list(vec![Value::Float(*re), Value::Float(*im)])
            })
            .collect()
    )
}

fn get_float(args: &[Value], idx: usize, fn_name: &str) -> GravResult<f64> {
    args.get(idx)
        .and_then(|v| v.as_float())
        .ok_or_else(|| runtime_err!("{fn_name}: expected number at position {idx}"))
}

fn require_list(args: &[Value], idx: usize, fn_name: &str) -> GravResult<Vec<f64>> {
    let val = args.get(idx)
        .ok_or_else(|| runtime_err!("{fn_name}: expected list at position {idx}"))?;
    to_f64_list(val)
        .ok_or_else(|| runtime_err!("{fn_name}: argument at position {idx} must be a list of numbers"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn call_transforms_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    let v = match name {
        "fft" => {
            let signal = require_list(args, 0, "fft")?;
            if signal.is_empty() {
                return Ok(Some(Value::make_list(vec![])));
            }
            // Zero-pad to next power of 2
            let n = next_power_of_2(signal.len());
            let mut re: Vec<f64> = signal;
            re.resize(n, 0.0);
            let mut im = vec![0.0; n];
            fft_iterative(&mut re, &mut im, false);
            let pairs: Vec<(f64, f64)> = re.into_iter().zip(im.into_iter()).collect();
            from_complex_list(&pairs)
        }

        "ifft" => {
            let val = args.first()
                .ok_or_else(|| runtime_err!("ifft: expected a list of [re, im] pairs"))?;
            let complex = to_complex_list(val)
                .ok_or_else(|| runtime_err!("ifft: argument must be a list of [re, im] pairs"))?;
            if complex.is_empty() {
                return Ok(Some(Value::make_list(vec![])));
            }
            let n = complex.len();
            if n & (n - 1) != 0 {
                return Err(runtime_err!("ifft: input length must be a power of 2, got {}", n));
            }
            let mut re: Vec<f64> = complex.iter().map(|(r, _)| *r).collect();
            let mut im: Vec<f64> = complex.iter().map(|(_, i)| *i).collect();
            fft_iterative(&mut re, &mut im, true);
            // Return real parts
            Value::make_list(re.into_iter().map(Value::Float).collect())
        }

        "dft" => {
            let signal = require_list(args, 0, "dft")?;
            let n = signal.len();
            if n == 0 {
                return Ok(Some(Value::make_list(vec![])));
            }
            let mut result: Vec<(f64, f64)> = Vec::with_capacity(n);
            for k in 0..n {
                let mut re = 0.0;
                let mut im = 0.0;
                for (j, &x) in signal.iter().enumerate() {
                    let angle = -2.0 * std::f64::consts::PI * k as f64 * j as f64 / n as f64;
                    re += x * angle.cos();
                    im += x * angle.sin();
                }
                result.push((re, im));
            }
            from_complex_list(&result)
        }

        "convolve" => {
            let a = require_list(args, 0, "convolve")?;
            let b = require_list(args, 1, "convolve")?;
            if a.is_empty() || b.is_empty() {
                return Ok(Some(Value::make_list(vec![])));
            }
            let out_len = a.len() + b.len() - 1;
            let mut result = vec![0.0; out_len];
            for (i, &ai) in a.iter().enumerate() {
                for (j, &bj) in b.iter().enumerate() {
                    result[i + j] += ai * bj;
                }
            }
            Value::make_list(result.into_iter().map(Value::Float).collect())
        }

        "laplace_eval" => {
            let num_coeffs = require_list(args, 0, "laplace_eval")?;
            let den_coeffs = require_list(args, 1, "laplace_eval")?;
            let s = get_float(args, 2, "laplace_eval")?;
            if den_coeffs.is_empty() {
                return Err(runtime_err!("laplace_eval: denominator coefficients must not be empty"));
            }
            // Evaluate polynomial: coeffs[0]*s^(n-1) + coeffs[1]*s^(n-2) + ... + coeffs[n-1]
            // Using Horner's method
            let num_val = horner_eval(&num_coeffs, s);
            let den_val = horner_eval(&den_coeffs, s);
            if den_val.abs() < 1e-15 {
                return Err(runtime_err!("laplace_eval: division by zero at s={}", s));
            }
            Value::Float(num_val / den_val)
        }

        // ── Unknown — let the caller decide ─────────────────────────────────
        _ => return Ok(None),
    };
    Ok(Some(v))
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal algorithms
// ─────────────────────────────────────────────────────────────────────────────

/// Next power of 2 >= n.
fn next_power_of_2(n: usize) -> usize {
    let mut p = 1;
    while p < n {
        p <<= 1;
    }
    p
}

/// Iterative Cooley-Tukey FFT (radix-2 DIT).
/// If `inverse` is true, computes the inverse FFT (divides result by n).
fn fft_iterative(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    if n <= 1 { return; }
    debug_assert!(n & (n - 1) == 0, "FFT length must be a power of 2");

    // Bit-reversal permutation
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    // Butterfly operations
    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let half = len / 2;
        let angle = sign * 2.0 * std::f64::consts::PI / len as f64;
        let w_re = angle.cos();
        let w_im = angle.sin();

        let mut i = 0;
        while i < n {
            let mut cur_re = 1.0;
            let mut cur_im = 0.0;
            for k in 0..half {
                let even = i + k;
                let odd = i + k + half;
                // t = w * x[odd]
                let t_re = cur_re * re[odd] - cur_im * im[odd];
                let t_im = cur_re * im[odd] + cur_im * re[odd];
                re[odd] = re[even] - t_re;
                im[odd] = im[even] - t_im;
                re[even] += t_re;
                im[even] += t_im;
                // Update twiddle factor
                let new_re = cur_re * w_re - cur_im * w_im;
                let new_im = cur_re * w_im + cur_im * w_re;
                cur_re = new_re;
                cur_im = new_im;
            }
            i += len;
        }
        len <<= 1;
    }

    // For inverse FFT, divide by n
    if inverse {
        let inv_n = 1.0 / n as f64;
        for i in 0..n {
            re[i] *= inv_n;
            im[i] *= inv_n;
        }
    }
}

/// Horner's method for polynomial evaluation.
/// coeffs = [a_n, a_{n-1}, ..., a_1, a_0]
/// Result = a_n * s^n + a_{n-1} * s^{n-1} + ... + a_0
fn horner_eval(coeffs: &[f64], s: f64) -> f64 {
    let mut result = 0.0;
    for &c in coeffs {
        result = result * s + c;
    }
    result
}
