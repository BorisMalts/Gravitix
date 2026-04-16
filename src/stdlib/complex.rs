use crate::value::Value;
use crate::error::GravResult;
use crate::runtime_err;

// ─────────────────────────────────────────────────────────────────────────────
// Complex number operations
//
// Works with Value::Complex(re, im) — being added by another agent.
// Also promotes Int/Float to complex (re, 0.0) transparently.
// ─────────────────────────────────────────────────────────────────────────────

fn as_complex(v: &Value) -> Option<(f64, f64)> {
    match v {
        Value::Complex(re, im) => Some((*re, *im)),
        Value::Float(f) => Some((*f, 0.0)),
        Value::Int(n) => Some((*n as f64, 0.0)),
        _ => None,
    }
}

/// Complex multiplication: (a + bi)(c + di)
fn cmul(a: f64, b: f64, c: f64, d: f64) -> (f64, f64) {
    (a * c - b * d, a * d + b * c)
}

/// Complex division: (a + bi) / (c + di)
fn cdiv(a: f64, b: f64, c: f64, d: f64) -> (f64, f64) {
    let denom = c * c + d * d;
    ((a * c + b * d) / denom, (b * c - a * d) / denom)
}

/// Complex natural logarithm: ln|z| + i*arg(z)
fn clog_impl(re: f64, im: f64) -> (f64, f64) {
    let modulus = (re * re + im * im).sqrt();
    let argument = im.atan2(re);
    (modulus.ln(), argument)
}

/// Complex exponential: e^z = e^re * (cos(im) + i*sin(im))
fn cexp_impl(re: f64, im: f64) -> (f64, f64) {
    let r = re.exp();
    (r * im.cos(), r * im.sin())
}

// ─────────────────────────────────────────────────────────────────────────────
// Argument extraction helpers
// ─────────────────────────────────────────────────────────────────────────────

fn require_complex(args: &[Value], idx: usize, fn_name: &str) -> GravResult<(f64, f64)> {
    args.get(idx)
        .and_then(|v| as_complex(v))
        .ok_or_else(|| runtime_err!("{fn_name}: expected complex/number at position {idx}"))
}

fn require_float(args: &[Value], idx: usize, fn_name: &str) -> GravResult<f64> {
    args.get(idx)
        .and_then(|v| v.as_float())
        .ok_or_else(|| runtime_err!("{fn_name}: expected number at position {idx}"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn call_complex_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    let v = match name {

        // ── Constructor ─────────────────────────────────────────────────────
        "complex" => {
            let re = require_float(args, 0, "complex")?;
            let im = require_float(args, 1, "complex")?;
            Value::Complex(re, im)
        }

        // ── Accessors ───────────────────────────────────────────────────────
        "re" => {
            let (re, _) = require_complex(args, 0, "re")?;
            Value::Float(re)
        }
        "im" => {
            let (_, im) = require_complex(args, 0, "im")?;
            Value::Float(im)
        }

        // ── Conjugate ───────────────────────────────────────────────────────
        "conj" => {
            let (re, im) = require_complex(args, 0, "conj")?;
            Value::Complex(re, -im)
        }

        // ── Modulus (absolute value) ────────────────────────────────────────
        "cabs" => {
            let (re, im) = require_complex(args, 0, "cabs")?;
            Value::Float((re * re + im * im).sqrt())
        }

        // ── Argument (phase angle) ──────────────────────────────────────────
        "arg" => {
            let (re, im) = require_complex(args, 0, "arg")?;
            Value::Float(im.atan2(re))
        }

        // ── Polar → Complex ─────────────────────────────────────────────────
        "polar" => {
            let r = require_float(args, 0, "polar")?;
            let theta = require_float(args, 1, "polar")?;
            Value::Complex(r * theta.cos(), r * theta.sin())
        }

        // ── Complex exponential ─────────────────────────────────────────────
        "cexp" => {
            let (re, im) = require_complex(args, 0, "cexp")?;
            let (er, ei) = cexp_impl(re, im);
            Value::Complex(er, ei)
        }

        // ── Complex logarithm ───────────────────────────────────────────────
        "clog" => {
            let (re, im) = require_complex(args, 0, "clog")?;
            if re == 0.0 && im == 0.0 {
                return Err(runtime_err!("clog: logarithm of zero is undefined"));
            }
            let (lr, li) = clog_impl(re, im);
            Value::Complex(lr, li)
        }

        // ── Complex sine ────────────────────────────────────────────────────
        // sin(a + bi) = sin(a)cosh(b) + i*cos(a)sinh(b)
        "csin" => {
            let (a, b) = require_complex(args, 0, "csin")?;
            Value::Complex(a.sin() * b.cosh(), a.cos() * b.sinh())
        }

        // ── Complex cosine ──────────────────────────────────────────────────
        // cos(a + bi) = cos(a)cosh(b) - i*sin(a)sinh(b)
        "ccos" => {
            let (a, b) = require_complex(args, 0, "ccos")?;
            Value::Complex(a.cos() * b.cosh(), -(a.sin() * b.sinh()))
        }

        // ── Complex tangent ─────────────────────────────────────────────────
        // tan(z) = sin(z) / cos(z)
        "ctan" => {
            let (a, b) = require_complex(args, 0, "ctan")?;
            let sin_re = a.sin() * b.cosh();
            let sin_im = a.cos() * b.sinh();
            let cos_re = a.cos() * b.cosh();
            let cos_im = -(a.sin() * b.sinh());
            let denom = cos_re * cos_re + cos_im * cos_im;
            if denom.abs() < 1e-300 {
                return Err(runtime_err!("ctan: division by zero (cos(z) = 0)"));
            }
            let (tr, ti) = cdiv(sin_re, sin_im, cos_re, cos_im);
            Value::Complex(tr, ti)
        }

        // ── Complex power ───────────────────────────────────────────────────
        // z^w = exp(w * log(z))
        "cpow" => {
            let (zr, zi) = require_complex(args, 0, "cpow")?;
            let (wr, wi) = require_complex(args, 1, "cpow")?;
            if zr == 0.0 && zi == 0.0 {
                if wr > 0.0 || (wr == 0.0 && wi != 0.0) {
                    Value::Complex(0.0, 0.0)
                } else {
                    return Err(runtime_err!("cpow: 0^w undefined for non-positive real part of w"));
                }
            } else {
                let (lr, li) = clog_impl(zr, zi);
                // w * log(z)
                let (pr, pi) = cmul(wr, wi, lr, li);
                let (er, ei) = cexp_impl(pr, pi);
                Value::Complex(er, ei)
            }
        }

        // ── Complex square root (principal) ─────────────────────────────────
        // sqrt(z) = sqrt(|z|) * exp(i * arg(z) / 2)
        "csqrt" => {
            let (re, im) = require_complex(args, 0, "csqrt")?;
            let modulus = (re * re + im * im).sqrt();
            let r = modulus.sqrt();
            let theta = im.atan2(re) / 2.0;
            Value::Complex(r * theta.cos(), r * theta.sin())
        }

        // ── Mobius transform ────────────────────────────────────────────────
        // (a*z + b) / (c*z + d)
        "mobius" => {
            let (ar, ai) = require_complex(args, 0, "mobius")?;
            let (br, bi) = require_complex(args, 1, "mobius")?;
            let (cr, ci) = require_complex(args, 2, "mobius")?;
            let (dr, di) = require_complex(args, 3, "mobius")?;
            let (zr, zi) = require_complex(args, 4, "mobius")?;
            // numerator = a*z + b
            let (azr, azi) = cmul(ar, ai, zr, zi);
            let num_re = azr + br;
            let num_im = azi + bi;
            // denominator = c*z + d
            let (czr, czi) = cmul(cr, ci, zr, zi);
            let den_re = czr + dr;
            let den_im = czi + di;
            let denom_sq = den_re * den_re + den_im * den_im;
            if denom_sq.abs() < 1e-300 {
                return Err(runtime_err!("mobius: denominator is zero"));
            }
            let (rr, ri) = cdiv(num_re, num_im, den_re, den_im);
            Value::Complex(rr, ri)
        }

        // ── Numerical residue ───────────────────────────────────────────────
        // Requires runtime function support — not yet available.
        "residue" => {
            // TODO: Needs runtime function passing support to evaluate f(z)
            // along a contour. Returns null for now.
            Value::Null
        }

        // ── Contour integral ────────────────────────────────────────────────
        // Requires runtime function support — not yet available.
        "contour_integral" => {
            // TODO: Needs runtime function passing support to evaluate f(z)
            // along a contour path. Returns null for now.
            Value::Null
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

    fn cx(re: f64, im: f64) -> Value { Value::Complex(re, im) }
    fn fl(f: f64) -> Value { Value::Float(f) }

    fn approx(a: f64, b: f64) -> bool { (a - b).abs() < 1e-9 }

    fn as_cx(v: &Value) -> (f64, f64) {
        match v {
            Value::Complex(r, i) => (*r, *i),
            _ => panic!("expected complex, got {:?}", v),
        }
    }

    #[test]
    fn test_complex_constructor() {
        let r = call_complex_builtin("complex", &[fl(3.0), fl(4.0)]).unwrap().unwrap();
        let (re, im) = as_cx(&r);
        assert!(approx(re, 3.0));
        assert!(approx(im, 4.0));
    }

    #[test]
    fn test_re_im() {
        let z = cx(3.0, 4.0);
        let re = call_complex_builtin("re", &[z.clone()]).unwrap().unwrap();
        let im = call_complex_builtin("im", &[z]).unwrap().unwrap();
        assert_eq!(re.as_float(), Some(3.0));
        assert_eq!(im.as_float(), Some(4.0));
    }

    #[test]
    fn test_conj() {
        let r = call_complex_builtin("conj", &[cx(3.0, 4.0)]).unwrap().unwrap();
        let (re, im) = as_cx(&r);
        assert!(approx(re, 3.0));
        assert!(approx(im, -4.0));
    }

    #[test]
    fn test_cabs() {
        let r = call_complex_builtin("cabs", &[cx(3.0, 4.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 5.0));
    }

    #[test]
    fn test_arg() {
        let r = call_complex_builtin("arg", &[cx(0.0, 1.0)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), std::f64::consts::FRAC_PI_2));
    }

    #[test]
    fn test_polar() {
        let r = call_complex_builtin("polar", &[fl(1.0), fl(std::f64::consts::FRAC_PI_2)])
            .unwrap().unwrap();
        let (re, im) = as_cx(&r);
        assert!(approx(re, 0.0));
        assert!(approx(im, 1.0));
    }

    #[test]
    fn test_cexp() {
        // e^(i*pi) = -1 + 0i
        let r = call_complex_builtin("cexp", &[cx(0.0, std::f64::consts::PI)])
            .unwrap().unwrap();
        let (re, im) = as_cx(&r);
        assert!(approx(re, -1.0));
        assert!(im.abs() < 1e-9);
    }

    #[test]
    fn test_clog() {
        // log(e) = 1 + 0i
        let r = call_complex_builtin("clog", &[cx(std::f64::consts::E, 0.0)])
            .unwrap().unwrap();
        let (re, im) = as_cx(&r);
        assert!(approx(re, 1.0));
        assert!(im.abs() < 1e-9);
    }

    #[test]
    fn test_clog_zero_error() {
        let r = call_complex_builtin("clog", &[cx(0.0, 0.0)]);
        assert!(r.is_err());
    }

    #[test]
    fn test_csin() {
        // sin(0) = 0
        let r = call_complex_builtin("csin", &[cx(0.0, 0.0)]).unwrap().unwrap();
        let (re, im) = as_cx(&r);
        assert!(re.abs() < 1e-9);
        assert!(im.abs() < 1e-9);
    }

    #[test]
    fn test_ccos() {
        // cos(0) = 1
        let r = call_complex_builtin("ccos", &[cx(0.0, 0.0)]).unwrap().unwrap();
        let (re, im) = as_cx(&r);
        assert!(approx(re, 1.0));
        assert!(im.abs() < 1e-9);
    }

    #[test]
    fn test_csqrt() {
        // sqrt(4) = 2
        let r = call_complex_builtin("csqrt", &[cx(4.0, 0.0)]).unwrap().unwrap();
        let (re, im) = as_cx(&r);
        assert!(approx(re, 2.0));
        assert!(im.abs() < 1e-9);
    }

    #[test]
    fn test_cpow() {
        // (1+i)^2 = 2i
        let r = call_complex_builtin("cpow", &[cx(1.0, 1.0), cx(2.0, 0.0)])
            .unwrap().unwrap();
        let (re, im) = as_cx(&r);
        assert!(re.abs() < 1e-9);
        assert!(approx(im, 2.0));
    }

    #[test]
    fn test_mobius_identity() {
        // Identity transform: (1*z + 0) / (0*z + 1) = z
        let r = call_complex_builtin("mobius", &[
            cx(1.0, 0.0), cx(0.0, 0.0),
            cx(0.0, 0.0), cx(1.0, 0.0),
            cx(3.0, 4.0),
        ]).unwrap().unwrap();
        let (re, im) = as_cx(&r);
        assert!(approx(re, 3.0));
        assert!(approx(im, 4.0));
    }

    #[test]
    fn test_int_promoted_to_complex() {
        let r = call_complex_builtin("cabs", &[Value::Int(5)]).unwrap().unwrap();
        assert!(approx(r.as_float().unwrap(), 5.0));
    }

    #[test]
    fn test_unknown_returns_none() {
        assert!(call_complex_builtin("unknown_fn", &[]).unwrap().is_none());
    }

    #[test]
    fn test_residue_returns_null() {
        let r = call_complex_builtin("residue", &[]).unwrap().unwrap();
        assert!(matches!(r, Value::Null));
    }

    #[test]
    fn test_contour_integral_returns_null() {
        let r = call_complex_builtin("contour_integral", &[]).unwrap().unwrap();
        assert!(matches!(r, Value::Null));
    }
}
