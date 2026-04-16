use crate::value::Value;
use crate::error::GravResult;
use crate::runtime_err;

// ─────────────────────────────────────────────────────────────────────────────
// Numerical calculus functions
//
// All functions work with pre-computed lists of values since the builtin
// system cannot pass function closures.
// ─────────────────────────────────────────────────────────────────────────────

// ── Argument extraction helpers ─────────────────────────────────────────────

fn require_float(args: &[Value], idx: usize, fn_name: &str) -> GravResult<f64> {
    args.get(idx)
        .and_then(|v| v.as_float())
        .ok_or_else(|| runtime_err!("{fn_name}: expected number at position {idx}"))
}

fn require_float_list(args: &[Value], idx: usize, fn_name: &str) -> GravResult<Vec<f64>> {
    let list = match args.get(idx) {
        Some(Value::List(l)) => l.borrow().clone(),
        _ => return Err(runtime_err!("{fn_name}: expected list at position {idx}")),
    };
    list.iter()
        .enumerate()
        .map(|(i, v)| {
            v.as_float()
                .ok_or_else(|| runtime_err!("{fn_name}: element {i} of list is not a number"))
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn call_calculus_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    let v = match name {

        // ── First derivative (central difference) ───────────────────────────
        // deriv([y0, y1, y2], h) → (y2 - y0) / (2h)
        // Three sample points: y0=f(x-h), y1=f(x), y2=f(x+h)
        "deriv" => {
            let ys = require_float_list(args, 0, "deriv")?;
            let h = require_float(args, 1, "deriv")?;
            if ys.len() < 2 {
                return Err(runtime_err!("deriv: need at least 2 values"));
            }
            if h.abs() < 1e-300 {
                return Err(runtime_err!("deriv: step h must be non-zero"));
            }
            let result = if ys.len() == 2 {
                // Forward difference: (y1 - y0) / h
                (ys[1] - ys[0]) / h
            } else {
                // Central difference: (y2 - y0) / (2h)
                (ys[2] - ys[0]) / (2.0 * h)
            };
            Value::Float(result)
        }

        // ── Second derivative ───────────────────────────────────────────────
        // deriv2(y0, y1, y2, h) → (y0 - 2*y1 + y2) / h^2
        "deriv2" => {
            let y0 = require_float(args, 0, "deriv2")?;
            let y1 = require_float(args, 1, "deriv2")?;
            let y2 = require_float(args, 2, "deriv2")?;
            let h = require_float(args, 3, "deriv2")?;
            if h.abs() < 1e-300 {
                return Err(runtime_err!("deriv2: step h must be non-zero"));
            }
            Value::Float((y0 - 2.0 * y1 + y2) / (h * h))
        }

        // ── Trapezoidal integration ─────────────────────────────────────────
        // integral_trapz(ys, h) → h * ( y0/2 + y1 + y2 + ... + yn/2 )
        "integral_trapz" => {
            let ys = require_float_list(args, 0, "integral_trapz")?;
            let h = require_float(args, 1, "integral_trapz")?;
            if ys.len() < 2 {
                return Err(runtime_err!("integral_trapz: need at least 2 values"));
            }
            let n = ys.len() - 1;
            let mut sum = (ys[0] + ys[n]) / 2.0;
            for y in &ys[1..n] {
                sum += y;
            }
            Value::Float(sum * h)
        }

        // ── Simpson's rule integration ──────────────────────────────────────
        // Requires an odd number of points (even number of intervals)
        "integral_simpson" => {
            let ys = require_float_list(args, 0, "integral_simpson")?;
            let h = require_float(args, 1, "integral_simpson")?;
            if ys.len() < 3 {
                return Err(runtime_err!("integral_simpson: need at least 3 values"));
            }
            let n = ys.len() - 1;
            if n % 2 != 0 {
                return Err(runtime_err!(
                    "integral_simpson: need odd number of points (even intervals), got {} points",
                    ys.len()
                ));
            }
            let mut sum = ys[0] + ys[n];
            for i in (1..n).step_by(2) {
                sum += 4.0 * ys[i];
            }
            for i in (2..n).step_by(2) {
                sum += 2.0 * ys[i];
            }
            Value::Float(sum * h / 3.0)
        }

        // ── Finite differences ──────────────────────────────────────────────
        // diff([a, b, c, ...]) → [b-a, c-b, ...]
        "diff" => {
            let ys = require_float_list(args, 0, "diff")?;
            if ys.is_empty() {
                return Ok(Some(Value::make_list(vec![])));
            }
            let diffs: Vec<Value> = ys.windows(2)
                .map(|w| Value::Float(w[1] - w[0]))
                .collect();
            Value::make_list(diffs)
        }

        // ── Cumulative sum ──────────────────────────────────────────────────
        "cumsum" => {
            let ys = require_float_list(args, 0, "cumsum")?;
            let mut acc = 0.0;
            let result: Vec<Value> = ys.iter().map(|&y| {
                acc += y;
                Value::Float(acc)
            }).collect();
            Value::make_list(result)
        }

        // ── Cumulative product ──────────────────────────────────────────────
        "cumprod" => {
            let ys = require_float_list(args, 0, "cumprod")?;
            let mut acc = 1.0;
            let result: Vec<Value> = ys.iter().map(|&y| {
                acc *= y;
                Value::Float(acc)
            }).collect();
            Value::make_list(result)
        }

        // ── Newton-Raphson step ─────────────────────────────────────────────
        // newton_step(fx, dfx, x) → x - fx/dfx
        "newton_step" => {
            let fx = require_float(args, 0, "newton_step")?;
            let dfx = require_float(args, 1, "newton_step")?;
            let x = require_float(args, 2, "newton_step")?;
            if dfx.abs() < 1e-300 {
                return Err(runtime_err!("newton_step: derivative is zero, cannot proceed"));
            }
            Value::Float(x - fx / dfx)
        }

        // ── Bisection step ──────────────────────────────────────────────────
        // bisect_step(a, b, fa, fb) → returns map { mid, a, b, side }
        "bisect_step" => {
            let a = require_float(args, 0, "bisect_step")?;
            let b = require_float(args, 1, "bisect_step")?;
            let fa = require_float(args, 2, "bisect_step")?;
            let fb = require_float(args, 3, "bisect_step")?;
            if fa * fb > 0.0 {
                return Err(runtime_err!(
                    "bisect_step: f(a) and f(b) must have opposite signs"
                ));
            }
            let mid = (a + b) / 2.0;
            // Caller provides f(mid) in the next call; we return the midpoint
            // and indicate which side based on sign convention.
            // Since we can't evaluate f(mid) here, we return the midpoint
            // and both endpoints for the caller to decide.
            let mut m = std::collections::HashMap::new();
            m.insert("mid".to_string(), Value::Float(mid));
            m.insert("a".to_string(), Value::Float(a));
            m.insert("b".to_string(), Value::Float(b));
            m.insert("fa".to_string(), Value::Float(fa));
            m.insert("fb".to_string(), Value::Float(fb));
            Value::make_map(m)
        }

        // ── Taylor polynomial evaluation ────────────────────────────────────
        // taylor_eval(coeffs, x, x0) → Σ coeffs[i] * (x - x0)^i
        "taylor_eval" => {
            let coeffs = require_float_list(args, 0, "taylor_eval")?;
            let x = require_float(args, 1, "taylor_eval")?;
            let x0 = args.get(2).and_then(|v| v.as_float()).unwrap_or(0.0);
            let dx = x - x0;
            // Horner's method (evaluate from highest to lowest)
            let mut result = 0.0;
            for c in coeffs.iter().rev() {
                result = result * dx + c;
            }
            Value::Float(result)
        }

        // ── Sum of a list ───────────────────────────────────────────────────
        "sigma" => {
            let ys = require_float_list(args, 0, "sigma")?;
            Value::Float(ys.iter().sum())
        }

        // ── Product of a list ───────────────────────────────────────────────
        "product" => {
            let ys = require_float_list(args, 0, "product")?;
            Value::Float(ys.iter().product())
        }

        // ── 2D gradient vector ──────────────────────────────────────────────
        // gradient_2d(fx, fy) → [fx, fy]
        "gradient_2d" => {
            let fx = require_float(args, 0, "gradient_2d")?;
            let fy = require_float(args, 1, "gradient_2d")?;
            Value::make_list(vec![Value::Float(fx), Value::Float(fy)])
        }

        // ── Sequence limit estimation ───────────────────────────────────────
        // Uses Richardson extrapolation on last 3 elements if available,
        // otherwise returns the last element.
        "limit_seq" => {
            let ys = require_float_list(args, 0, "limit_seq")?;
            if ys.is_empty() {
                return Err(runtime_err!("limit_seq: empty list"));
            }
            let n = ys.len();
            if n < 3 {
                // Not enough points for extrapolation, return last element
                Value::Float(ys[n - 1])
            } else {
                // Aitken's delta-squared (Richardson-like) acceleration:
                // s* = s_n - (s_n - s_{n-1})^2 / (s_n - 2*s_{n-1} + s_{n-2})
                let s0 = ys[n - 3];
                let s1 = ys[n - 2];
                let s2 = ys[n - 1];
                let denom = s2 - 2.0 * s1 + s0;
                if denom.abs() < 1e-300 {
                    // Sequence may have already converged
                    Value::Float(s2)
                } else {
                    let diff = s2 - s1;
                    let accelerated = s2 - (diff * diff) / denom;
                    Value::Float(accelerated)
                }
            }
        }

        // ── Romberg integration ─────────────────────────────────────────────
        // romberg(ys, h, levels) → improved integral estimate
        // Uses the trapezoidal rule as base and Richardson extrapolation.
        // `ys` must have 2^k + 1 points for the deepest level.
        "romberg" => {
            let ys = require_float_list(args, 0, "romberg")?;
            let h = require_float(args, 1, "romberg")?;
            let max_levels = args.get(2)
                .and_then(|v| v.as_int())
                .unwrap_or(4) as usize;

            if ys.len() < 2 {
                return Err(runtime_err!("romberg: need at least 2 values"));
            }

            // Build trapezoidal estimates at successively finer grids
            let n = ys.len() - 1;
            let mut r: Vec<Vec<f64>> = Vec::new();

            // Determine how many levels we can actually compute
            let mut levels = 0usize;
            let mut step = 1usize;
            while step <= n && levels < max_levels {
                levels += 1;
                step *= 2;
            }
            if levels == 0 {
                // Fallback: basic trapezoidal
                let sum = (ys[0] + ys[n]) / 2.0;
                return Ok(Some(Value::Float(sum * h * n as f64)));
            }

            // Level 0: compute trapezoidal estimates at different step sizes
            step = n; // coarsest
            for k in 0..levels {
                let current_step = if k == 0 { n } else { n / (1 << k) };
                if current_step == 0 { break; }
                let actual_step = n / current_step; // stride in the ys array
                let local_h = h * actual_step as f64;

                let mut sum = (ys[0] + ys[n]) / 2.0;
                let mut idx = actual_step;
                while idx < n {
                    sum += ys[idx];
                    idx += actual_step;
                }
                let trap = sum * local_h;

                if r.is_empty() {
                    r.push(vec![trap]);
                } else {
                    r.push(vec![trap]);
                }
            }

            // Richardson extrapolation: R[k][j] = (4^j * R[k][j-1] - R[k-1][j-1]) / (4^j - 1)
            for j in 1..r.len() {
                for k in j..r.len() {
                    let factor = (4.0_f64).powi(j as i32);
                    let prev_this = r[k][j - 1];
                    let prev_prev = r[k - 1][j - 1];
                    let val = (factor * prev_this - prev_prev) / (factor - 1.0);
                    r[k].push(val);
                }
            }

            // Best estimate is the last element of the last row
            let last_row = &r[r.len() - 1];
            let best = last_row[last_row.len() - 1];
            Value::Float(best)
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
    fn list(vals: &[f64]) -> Value {
        Value::make_list(vals.iter().map(|&v| Value::Float(v)).collect())
    }
    fn approx(a: f64, b: f64) -> bool { (a - b).abs() < 1e-6 }

    #[test]
    fn test_deriv_central() {
        // f(x) = x^2, x=2, h=0.01
        // f(1.99) = 3.9601, f(2.0) = 4.0, f(2.01) = 4.0401
        let r = call_calculus_builtin("deriv", &[list(&[3.9601, 4.0, 4.0401]), fl(0.01)])
            .unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 4.0));
    }

    #[test]
    fn test_deriv_forward() {
        // Forward difference: (4.0 - 3.96) / 0.02 = 2.0 (roughly)
        let r = call_calculus_builtin("deriv", &[list(&[0.0, 1.0]), fl(1.0)])
            .unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 1.0));
    }

    #[test]
    fn test_deriv2() {
        // f(x) = x^2 → f''(x) = 2 everywhere
        // y0 = (1)^2 = 1, y1 = (2)^2 = 4, y2 = (3)^2 = 9, h = 1
        let r = call_calculus_builtin("deriv2", &[fl(1.0), fl(4.0), fl(9.0), fl(1.0)])
            .unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 2.0));
    }

    #[test]
    fn test_integral_trapz() {
        // Integrate f(x) = x from 0 to 4 with step 1: [0, 1, 2, 3, 4]
        // Exact: 8.0
        let r = call_calculus_builtin("integral_trapz", &[list(&[0.0, 1.0, 2.0, 3.0, 4.0]), fl(1.0)])
            .unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 8.0));
    }

    #[test]
    fn test_integral_simpson() {
        // Integrate f(x) = x^2 from 0 to 2, 5 points, h=0.5
        // Values: [0, 0.25, 1.0, 2.25, 4.0]
        // Exact: 8/3 ≈ 2.6667
        let r = call_calculus_builtin("integral_simpson", &[
            list(&[0.0, 0.25, 1.0, 2.25, 4.0]), fl(0.5)
        ]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 8.0 / 3.0));
    }

    #[test]
    fn test_diff() {
        let r = call_calculus_builtin("diff", &[list(&[1.0, 3.0, 6.0, 10.0])])
            .unwrap().unwrap();
        if let Value::List(l) = r {
            let l = l.borrow();
            assert_eq!(l.len(), 3);
            assert!(approx(l[0].as_float().unwrap(), 2.0));
            assert!(approx(l[1].as_float().unwrap(), 3.0));
            assert!(approx(l[2].as_float().unwrap(), 4.0));
        } else {
            panic!("expected list");
        }
    }

    #[test]
    fn test_cumsum() {
        let r = call_calculus_builtin("cumsum", &[list(&[1.0, 2.0, 3.0, 4.0])])
            .unwrap().unwrap();
        if let Value::List(l) = r {
            let l = l.borrow();
            assert_eq!(l.len(), 4);
            assert!(approx(l[0].as_float().unwrap(), 1.0));
            assert!(approx(l[1].as_float().unwrap(), 3.0));
            assert!(approx(l[2].as_float().unwrap(), 6.0));
            assert!(approx(l[3].as_float().unwrap(), 10.0));
        } else {
            panic!("expected list");
        }
    }

    #[test]
    fn test_cumprod() {
        let r = call_calculus_builtin("cumprod", &[list(&[1.0, 2.0, 3.0, 4.0])])
            .unwrap().unwrap();
        if let Value::List(l) = r {
            let l = l.borrow();
            assert!(approx(l[3].as_float().unwrap(), 24.0));
        } else {
            panic!("expected list");
        }
    }

    #[test]
    fn test_newton_step() {
        // f(x) = x^2 - 4, f'(x) = 2x, starting at x=3
        // Next: 3 - (9-4)/(6) = 3 - 5/6 ≈ 2.1667
        let r = call_calculus_builtin("newton_step", &[fl(5.0), fl(6.0), fl(3.0)])
            .unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 3.0 - 5.0 / 6.0));
    }

    #[test]
    fn test_bisect_step() {
        // f changes sign between a=1 (f=neg) and b=3 (f=pos)
        let r = call_calculus_builtin("bisect_step", &[fl(1.0), fl(3.0), fl(-1.0), fl(1.0)])
            .unwrap().unwrap();
        if let Value::Map(m) = r {
            let m = m.borrow();
            assert!(approx(m.get("mid").unwrap().as_float().unwrap(), 2.0));
        } else {
            panic!("expected map");
        }
    }

    #[test]
    fn test_taylor_eval() {
        // 1 + x + x^2/2 ≈ e^x Taylor at x0=0 for x=1 → 1 + 1 + 0.5 = 2.5
        let r = call_calculus_builtin("taylor_eval", &[list(&[1.0, 1.0, 0.5]), fl(1.0), fl(0.0)])
            .unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 2.5));
    }

    #[test]
    fn test_taylor_eval_shifted() {
        // Taylor around x0=1: coeffs [2.0, 3.0], eval at x=2 → 2 + 3*(2-1) = 5
        let r = call_calculus_builtin("taylor_eval", &[list(&[2.0, 3.0]), fl(2.0), fl(1.0)])
            .unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 5.0));
    }

    #[test]
    fn test_sigma() {
        let r = call_calculus_builtin("sigma", &[list(&[1.0, 2.0, 3.0, 4.0])])
            .unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 10.0));
    }

    #[test]
    fn test_product() {
        let r = call_calculus_builtin("product", &[list(&[1.0, 2.0, 3.0, 4.0])])
            .unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 24.0));
    }

    #[test]
    fn test_gradient_2d() {
        let r = call_calculus_builtin("gradient_2d", &[fl(3.0), fl(4.0)])
            .unwrap().unwrap();
        if let Value::List(l) = r {
            let l = l.borrow();
            assert_eq!(l.len(), 2);
            assert!(approx(l[0].as_float().unwrap(), 3.0));
            assert!(approx(l[1].as_float().unwrap(), 4.0));
        } else {
            panic!("expected list");
        }
    }

    #[test]
    fn test_limit_seq() {
        // Converging sequence: 1, 0.5, 0.25 → should accelerate toward 0
        let r = call_calculus_builtin("limit_seq", &[list(&[1.0, 0.5, 0.25])])
            .unwrap().unwrap();
        let val = r.as_float().unwrap();
        assert!(val.abs() < 0.1); // should be close to 0
    }

    #[test]
    fn test_romberg() {
        // Integrate f(x) = x from 0 to 4, 5 points, h=1
        let r = call_calculus_builtin("romberg", &[list(&[0.0, 1.0, 2.0, 3.0, 4.0]), fl(1.0), Value::Int(2)])
            .unwrap().unwrap();
        let val = r.as_float().unwrap();
        assert!(approx(val, 8.0));
    }

    #[test]
    fn test_unknown_returns_none() {
        assert!(call_calculus_builtin("unknown_fn", &[]).unwrap().is_none());
    }

    #[test]
    fn test_deriv_zero_step_error() {
        let r = call_calculus_builtin("deriv", &[list(&[1.0, 2.0, 3.0]), fl(0.0)]);
        assert!(r.is_err());
    }

    #[test]
    fn test_simpson_even_points_error() {
        // 4 points = 3 intervals (odd), should error
        let r = call_calculus_builtin("integral_simpson", &[
            list(&[0.0, 1.0, 2.0, 3.0]), fl(1.0)
        ]);
        assert!(r.is_err());
    }
}
