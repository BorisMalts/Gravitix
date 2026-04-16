use crate::value::Value;
use crate::error::GravResult;
use crate::runtime_err;

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn call_number_theory_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    let v = match name {
        "gcd" => {
            let a = get_int(args, 0, "gcd")?;
            let b = get_int(args, 1, "gcd")?;
            Value::Int(gcd(a.unsigned_abs(), b.unsigned_abs()) as i64)
        }

        "lcm" => {
            let a = get_int(args, 0, "lcm")?;
            let b = get_int(args, 1, "lcm")?;
            if a == 0 && b == 0 {
                Value::Int(0)
            } else {
                let g = gcd(a.unsigned_abs(), b.unsigned_abs());
                Value::Int((a.unsigned_abs() / g * b.unsigned_abs()) as i64)
            }
        }

        "factorial" => {
            let n = get_int(args, 0, "factorial")?;
            if n < 0 {
                return Err(runtime_err!("factorial: argument must be non-negative"));
            }
            if n > 20 {
                return Err(runtime_err!("factorial: argument too large (max 20 for i64)"));
            }
            let mut result: i64 = 1;
            for i in 2..=n {
                result *= i;
            }
            Value::Int(result)
        }

        "is_prime" => {
            let n = get_int(args, 0, "is_prime")?;
            Value::Bool(is_prime(n))
        }

        "primes" => {
            let n = get_int(args, 0, "primes")?;
            if n < 2 {
                Value::make_list(vec![])
            } else {
                let sieve = sieve_of_eratosthenes(n as usize);
                Value::make_list(sieve.into_iter().map(|p| Value::Int(p as i64)).collect())
            }
        }

        "prime_factors" => {
            let n = get_int(args, 0, "prime_factors")?;
            if n < 2 {
                Value::make_list(vec![])
            } else {
                let factors = prime_factorize(n);
                Value::make_list(factors.into_iter().map(Value::Int).collect())
            }
        }

        "divisors" => {
            let n = get_int(args, 0, "divisors")?;
            if n <= 0 {
                return Err(runtime_err!("divisors: argument must be positive"));
            }
            let mut divs = Vec::new();
            let limit = (n as f64).sqrt() as i64;
            for i in 1..=limit {
                if n % i == 0 {
                    divs.push(i);
                    if i != n / i {
                        divs.push(n / i);
                    }
                }
            }
            divs.sort();
            Value::make_list(divs.into_iter().map(Value::Int).collect())
        }

        "euler_phi" => {
            let n = get_int(args, 0, "euler_phi")?;
            if n <= 0 {
                return Err(runtime_err!("euler_phi: argument must be positive"));
            }
            Value::Int(euler_totient(n))
        }

        "modinv" => {
            let a = get_int(args, 0, "modinv")?;
            let m = get_int(args, 1, "modinv")?;
            if m <= 0 {
                return Err(runtime_err!("modinv: modulus must be positive"));
            }
            match mod_inverse(a, m) {
                Some(result) => Value::Int(result),
                None => return Err(runtime_err!("modinv: modular inverse does not exist (gcd({}, {}) != 1)", a, m)),
            }
        }

        "modpow" => {
            let base = get_int(args, 0, "modpow")?;
            let exp = get_int(args, 1, "modpow")?;
            let modulus = get_int(args, 2, "modpow")?;
            if modulus <= 0 {
                return Err(runtime_err!("modpow: modulus must be positive"));
            }
            if exp < 0 {
                return Err(runtime_err!("modpow: exponent must be non-negative"));
            }
            Value::Int(mod_pow(base, exp, modulus))
        }

        "fib" => {
            let n = get_int(args, 0, "fib")?;
            if n < 0 {
                return Err(runtime_err!("fib: argument must be non-negative"));
            }
            Value::Int(fibonacci(n))
        }

        "binomial" => {
            let n = get_int(args, 0, "binomial")?;
            let k = get_int(args, 1, "binomial")?;
            if n < 0 || k < 0 || k > n {
                Value::Int(0)
            } else {
                Value::Int(binomial_coeff(n, k))
            }
        }

        "perm" => {
            let n = get_int(args, 0, "perm")?;
            let k = get_int(args, 1, "perm")?;
            if n < 0 || k < 0 || k > n {
                Value::Int(0)
            } else {
                let mut result: i64 = 1;
                for i in (n - k + 1)..=n {
                    result *= i;
                }
                Value::Int(result)
            }
        }

        "stirling" => {
            let n = get_int(args, 0, "stirling")?;
            let k = get_int(args, 1, "stirling")?;
            if n < 0 || k < 0 {
                Value::Int(0)
            } else {
                Value::Int(stirling_second(n as usize, k as usize))
            }
        }

        // ── Unknown — let the caller decide ─────────────────────────────────
        _ => return Ok(None),
    };
    Ok(Some(v))
}

// ─────────────────────────────────────────────────────────────────────────────
// Argument extraction helper
// ─────────────────────────────────────────────────────────────────────────────

fn get_int(args: &[Value], idx: usize, fn_name: &str) -> GravResult<i64> {
    args.get(idx)
        .and_then(|v| v.as_int())
        .ok_or_else(|| runtime_err!("{fn_name}: expected integer at position {idx}"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal algorithms
// ─────────────────────────────────────────────────────────────────────────────

/// Euclidean algorithm for GCD on unsigned values.
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Trial division primality test with 6k +/- 1 optimization.
fn is_prime(n: i64) -> bool {
    if n <= 1 { return false; }
    if n <= 3 { return true; }
    if n % 2 == 0 || n % 3 == 0 { return false; }
    let mut i: i64 = 5;
    while i * i <= n {
        if n % i == 0 || n % (i + 2) == 0 {
            return false;
        }
        i += 6;
    }
    true
}

/// Sieve of Eratosthenes — returns sorted list of primes up to n.
fn sieve_of_eratosthenes(n: usize) -> Vec<usize> {
    let mut is_prime = vec![true; n + 1];
    is_prime[0] = false;
    if n >= 1 { is_prime[1] = false; }
    let mut i = 2;
    while i * i <= n {
        if is_prime[i] {
            let mut j = i * i;
            while j <= n {
                is_prime[j] = false;
                j += i;
            }
        }
        i += 1;
    }
    (2..=n).filter(|&i| is_prime[i]).collect()
}

/// Prime factorization — returns list of prime factors (with repetitions).
fn prime_factorize(mut n: i64) -> Vec<i64> {
    let mut factors = Vec::new();
    let mut d = 2i64;
    while d * d <= n {
        while n % d == 0 {
            factors.push(d);
            n /= d;
        }
        d += 1;
    }
    if n > 1 {
        factors.push(n);
    }
    factors
}

/// Euler's totient function.
fn euler_totient(mut n: i64) -> i64 {
    let mut result = n;
    let mut p = 2i64;
    while p * p <= n {
        if n % p == 0 {
            while n % p == 0 {
                n /= p;
            }
            result -= result / p;
        }
        p += 1;
    }
    if n > 1 {
        result -= result / n;
    }
    result
}

/// Extended Euclidean algorithm for modular inverse.
fn mod_inverse(a: i64, m: i64) -> Option<i64> {
    let (mut old_r, mut r) = (a, m);
    let (mut old_s, mut s) = (1i64, 0i64);
    while r != 0 {
        let q = old_r / r;
        let tmp_r = r;
        r = old_r - q * r;
        old_r = tmp_r;
        let tmp_s = s;
        s = old_s - q * s;
        old_s = tmp_s;
    }
    if old_r.abs() != 1 {
        return None; // gcd != 1, no inverse
    }
    Some(((old_s % m) + m) % m)
}

/// Binary modular exponentiation.
fn mod_pow(mut base: i64, mut exp: i64, modulus: i64) -> i64 {
    if modulus == 1 { return 0; }
    let mut result: i64 = 1;
    base = ((base % modulus) + modulus) % modulus;
    while exp > 0 {
        if exp % 2 == 1 {
            result = (result as i128 * base as i128 % modulus as i128) as i64;
        }
        exp >>= 1;
        base = (base as i128 * base as i128 % modulus as i128) as i64;
    }
    result
}

/// Iterative Fibonacci.
fn fibonacci(n: i64) -> i64 {
    if n <= 0 { return 0; }
    if n == 1 { return 1; }
    let (mut a, mut b) = (0i64, 1i64);
    for _ in 2..=n {
        let c = a + b;
        a = b;
        b = c;
    }
    b
}

/// Binomial coefficient C(n, k) — iterative to avoid overflow.
fn binomial_coeff(n: i64, k: i64) -> i64 {
    let k = k.min(n - k); // symmetry optimisation
    if k < 0 { return 0; }
    let mut result: i64 = 1;
    for i in 0..k {
        result = result * (n - i) / (i + 1);
    }
    result
}

/// Stirling numbers of the second kind S(n, k) — DP approach.
fn stirling_second(n: usize, k: usize) -> i64 {
    if k == 0 {
        return if n == 0 { 1 } else { 0 };
    }
    if k > n { return 0; }
    if k == 1 || k == n { return 1; }
    // Build a DP table
    // S(n, k) = k * S(n-1, k) + S(n-1, k-1)
    let mut prev = vec![0i64; k + 1];
    prev[0] = 1; // S(0, 0)
    for i in 1..=n {
        let mut curr = vec![0i64; k + 1];
        for j in 1..=k.min(i) {
            curr[j] = j as i64 * prev[j] + prev[j - 1];
        }
        prev = curr;
    }
    prev[k]
}
