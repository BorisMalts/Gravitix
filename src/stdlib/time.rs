use crate::value::Value;
use crate::error::GravResult;
use crate::runtime_err;

pub fn call_time_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    let v = match name {
        "now_unix" => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let ts = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64).unwrap_or(0);
            Value::Int(ts)
        }
        "now_str" => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let ts = SystemTime::now().duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs()).unwrap_or(0);
            let s  = secs_to_date(ts);
            Value::make_str(s)
        }
        "format_number" => {
            match args.first() {
                Some(Value::Int(n))   => Value::make_str(format_with_commas(*n)),
                Some(Value::Float(f)) => Value::make_str(format!("{f:.2}")),
                _ => return Err(runtime_err!("format_number: expected number")),
            }
        }
        "pad_left" => {
            let s     = args.first().map(|v| v.to_string()).unwrap_or_default();
            let width = args.get(1).and_then(|v| v.as_int()).unwrap_or(0) as usize;
            let pad   = args.get(2).map(|v| v.to_string()).unwrap_or_else(|| " ".to_string());
            let pad_ch = pad.chars().next().unwrap_or(' ');
            Value::make_str(format!("{:>width$}", s, width = width)
                .replacen(' ', &pad_ch.to_string(), 1))
        }
        "secs_to_date" => {
            let secs = args.first().and_then(|v| v.as_int()).unwrap_or(0) as u64;
            Value::make_str(secs_to_date(secs))
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}

fn format_with_commas(n: i64) -> String {
    let s = n.abs().to_string();
    let with_commas: String = s.chars().rev().enumerate()
        .flat_map(|(i, c)| if i > 0 && i % 3 == 0 { vec![',', c] } else { vec![c] })
        .collect::<String>()
        .chars().rev().collect();
    if n < 0 { format!("-{with_commas}") } else { with_commas }
}

pub fn secs_to_date(secs: u64) -> String {
    // Naive UTC calculation without chrono
    let days_total = secs / 86400;
    let time_secs  = secs % 86400;
    let h = time_secs / 3600;
    let m = (time_secs % 3600) / 60;
    let s = time_secs % 60;
    // Gregorian calendar approximation
    let y400 = days_total / 146097;
    let rem   = days_total % 146097;
    let y100  = (rem / 36524).min(3);
    let rem   = rem - y100 * 36524;
    let y4    = rem / 1461;
    let rem   = rem % 1461;
    let y1    = (rem / 365).min(3);
    let year  = 1970 + y400 * 400 + y100 * 100 + y4 * 4 + y1;
    let day_of_year = rem - y1 * 365 + 1;
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let months = if leap {
        &[31u64, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31u64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut day = day_of_year;
    let mut month = 1u64;
    for (i, &dm) in months.iter().enumerate() {
        if day <= dm { month = i as u64 + 1; break; }
        day -= dm;
    }
    format!("{year}-{month:02}-{day:02} {h:02}:{m:02}:{s:02} UTC")
}
