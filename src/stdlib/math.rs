use crate::value::Value;
use crate::error::GravResult;
use crate::runtime_err;

// ─────────────────────────────────────────────────────────────────────────────
// Golden ratio and Euler-Mascheroni constant (not in std::f64::consts)
// ─────────────────────────────────────────────────────────────────────────────

const PHI: f64 = 1.618_033_988_749_895;
const EULER_GAMMA: f64 = 0.577_215_664_901_532_9;

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point — dispatches `math.<name>(args)` calls
// ─────────────────────────────────────────────────────────────────────────────

pub fn call_math_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    let v = match name {

        // ── Constants ────────────────────────────────────────────────────────
        "pi"          => Value::Float(std::f64::consts::PI),
        "e"           => Value::Float(std::f64::consts::E),
        "tau"         => Value::Float(std::f64::consts::TAU),
        "inf"         => Value::Float(f64::INFINITY),
        "nan"         => Value::Float(f64::NAN),
        "phi"         => Value::Float(PHI),
        "euler_gamma" => Value::Float(EULER_GAMMA),

        // ── Basic arithmetic ─────────────────────────────────────────────────
        "abs" => {
            match args.first() {
                Some(Value::Int(n))   => Value::Int(n.abs()),
                Some(Value::Float(f)) => Value::Float(f.abs()),
                _ => return Err(runtime_err!("abs: expected number")),
            }
        }
        "min" => {
            let a = get_num(args, 0, "min")?;
            let b = get_num(args, 1, "min")?;
            if a <= b { args[0].clone() } else { args[1].clone() }
        }
        "max" => {
            let a = get_num(args, 0, "max")?;
            let b = get_num(args, 1, "max")?;
            if a >= b { args[0].clone() } else { args[1].clone() }
        }
        "floor" => { let f = get_float(args, 0, "floor")?; Value::Int(f.floor() as i64) }
        "ceil"  => { let f = get_float(args, 0, "ceil")?;  Value::Int(f.ceil()  as i64) }
        "round" => { let f = get_float(args, 0, "round")?; Value::Int(f.round() as i64) }
        "sqrt"  => { let f = get_float(args, 0, "sqrt")?;  Value::Float(f.sqrt()) }
        "pow" => {
            let base = get_float(args, 0, "pow")?;
            let exp  = get_float(args, 1, "pow")?;
            Value::Float(base.powf(exp))
        }
        "random" => {
            // Simple LCG random — no external deps needed
            use std::time::{SystemTime, UNIX_EPOCH};
            let seed = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as u64).unwrap_or(42);
            let max = args.first().and_then(|v| v.as_int()).unwrap_or(100);
            let r = (seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) >> 33) % max as u64;
            Value::Int(r as i64)
        }

        // ── Trigonometry (radians) ───────────────────────────────────────────
        "sin"  => { let f = get_float(args, 0, "sin")?;  Value::Float(f.sin())  }
        "cos"  => { let f = get_float(args, 0, "cos")?;  Value::Float(f.cos())  }
        "tan"  => { let f = get_float(args, 0, "tan")?;  Value::Float(f.tan())  }
        "asin" => { let f = get_float(args, 0, "asin")?; Value::Float(f.asin()) }
        "acos" => { let f = get_float(args, 0, "acos")?; Value::Float(f.acos()) }
        "atan" => { let f = get_float(args, 0, "atan")?; Value::Float(f.atan()) }
        "atan2" => {
            let y = get_float(args, 0, "atan2")?;
            let x = get_float(args, 1, "atan2")?;
            Value::Float(y.atan2(x))
        }

        // ── Hyperbolic ───────────────────────────────────────────────────────
        "sinh"  => { let f = get_float(args, 0, "sinh")?;  Value::Float(f.sinh())  }
        "cosh"  => { let f = get_float(args, 0, "cosh")?;  Value::Float(f.cosh())  }
        "tanh"  => { let f = get_float(args, 0, "tanh")?;  Value::Float(f.tanh())  }
        "asinh" => { let f = get_float(args, 0, "asinh")?; Value::Float(f.asinh()) }
        "acosh" => { let f = get_float(args, 0, "acosh")?; Value::Float(f.acosh()) }
        "atanh" => { let f = get_float(args, 0, "atanh")?; Value::Float(f.atanh()) }

        // ── Logarithms & exponentials ────────────────────────────────────────
        "ln"    => { let f = get_float(args, 0, "ln")?;    Value::Float(f.ln())    }
        "log2"  => { let f = get_float(args, 0, "log2")?;  Value::Float(f.log2())  }
        "log10" => { let f = get_float(args, 0, "log10")?; Value::Float(f.log10()) }
        "log" => {
            let base = get_float(args, 0, "log")?;
            let x    = get_float(args, 1, "log")?;
            Value::Float(x.log(base))
        }
        "exp"  => { let f = get_float(args, 0, "exp")?;  Value::Float(f.exp())  }
        "exp2" => { let f = get_float(args, 0, "exp2")?; Value::Float(f.exp2()) }

        // ── Helpers ──────────────────────────────────────────────────────────
        "sign" => {
            let f = get_float(args, 0, "sign")?;
            let s = if f > 0.0 { 1 } else if f < 0.0 { -1 } else { 0 };
            Value::Int(s)
        }
        "trunc" => { let f = get_float(args, 0, "trunc")?; Value::Int(f.trunc() as i64) }
        "fract" => { let f = get_float(args, 0, "fract")?; Value::Float(f.fract()) }
        "hypot" => {
            let a = get_float(args, 0, "hypot")?;
            let b = get_float(args, 1, "hypot")?;
            Value::Float(a.hypot(b))
        }
        "clamp" => {
            let val = get_float(args, 0, "clamp")?;
            let min = get_float(args, 1, "clamp")?;
            let max = get_float(args, 2, "clamp")?;
            Value::Float(val.clamp(min, max))
        }
        "lerp" => {
            let a = get_float(args, 0, "lerp")?;
            let b = get_float(args, 1, "lerp")?;
            let t = get_float(args, 2, "lerp")?;
            Value::Float(a + t * (b - a))
        }
        "map_range" => {
            let v       = get_float(args, 0, "map_range")?;
            let in_min  = get_float(args, 1, "map_range")?;
            let in_max  = get_float(args, 2, "map_range")?;
            let out_min = get_float(args, 3, "map_range")?;
            let out_max = get_float(args, 4, "map_range")?;
            let t = (v - in_min) / (in_max - in_min);
            Value::Float(out_min + t * (out_max - out_min))
        }
        "degrees" => {
            let rad = get_float(args, 0, "degrees")?;
            Value::Float(rad.to_degrees())
        }
        "radians" => {
            let deg = get_float(args, 0, "radians")?;
            Value::Float(deg.to_radians())
        }
        "cbrt" => { let f = get_float(args, 0, "cbrt")?; Value::Float(f.cbrt()) }
        "nroot" => {
            let x = get_float(args, 0, "nroot")?;
            let n = get_float(args, 1, "nroot")?;
            Value::Float(x.powf(1.0 / n))
        }
        "approx_eq" => {
            let a = get_float(args, 0, "approx_eq")?;
            let b = get_float(args, 1, "approx_eq")?;
            let eps = args.get(2)
                .and_then(|v| v.as_float())
                .unwrap_or(1e-9);
            Value::Bool((a - b).abs() < eps)
        }

        // ── Unknown — let the caller decide ──────────────────────────────────
        _ => return Ok(None),
    };
    Ok(Some(v))
}

// ─────────────────────────────────────────────────────────────────────────────
// Argument extraction helpers
// ─────────────────────────────────────────────────────────────────────────────

fn get_num(args: &[Value], idx: usize, fn_name: &str) -> GravResult<f64> {
    args.get(idx)
        .and_then(|v| v.as_float())
        .ok_or_else(|| crate::runtime_err!("{fn_name}: expected number at position {idx}"))
}

fn get_float(args: &[Value], idx: usize, fn_name: &str) -> GravResult<f64> {
    args.get(idx)
        .and_then(|v| v.as_float())
        .ok_or_else(|| crate::runtime_err!("{fn_name}: expected number at position {idx}"))
}
