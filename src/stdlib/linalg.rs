use crate::value::Value;
use crate::error::GravResult;
use crate::runtime_err;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers — convert between Value and Rust numeric types
// ─────────────────────────────────────────────────────────────────────────────

fn to_vec(v: &Value) -> Option<Vec<f64>> {
    if let Value::List(l) = v {
        l.borrow().iter().map(|v| v.as_float()).collect()
    } else {
        None
    }
}

fn to_matrix(v: &Value) -> Option<Vec<Vec<f64>>> {
    if let Value::List(l) = v {
        l.borrow().iter().map(|row| to_vec(row)).collect()
    } else {
        None
    }
}

fn from_vec(v: Vec<f64>) -> Value {
    Value::make_list(v.into_iter().map(Value::Float).collect())
}

fn from_matrix(m: Vec<Vec<f64>>) -> Value {
    Value::make_list(m.into_iter().map(|row| from_vec(row)).collect())
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

pub fn call_linalg_builtin(name: &str, args: &[Value]) -> GravResult<Option<Value>> {
    let v = match name {
        // ── Vector operations ───────────────────────────────────────────────
        "dot" => {
            let a = to_vec(args.first().ok_or_else(|| runtime_err!("dot: expected 2 vectors"))?)
                .ok_or_else(|| runtime_err!("dot: first arg must be a list of numbers"))?;
            let b = to_vec(args.get(1).ok_or_else(|| runtime_err!("dot: expected 2 vectors"))?)
                .ok_or_else(|| runtime_err!("dot: second arg must be a list of numbers"))?;
            if a.len() != b.len() {
                return Err(runtime_err!("dot: vectors must have the same length ({} vs {})", a.len(), b.len()));
            }
            let result: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
            Value::Float(result)
        }

        "cross" => {
            let a = to_vec(args.first().ok_or_else(|| runtime_err!("cross: expected 2 vectors"))?)
                .ok_or_else(|| runtime_err!("cross: first arg must be a list of numbers"))?;
            let b = to_vec(args.get(1).ok_or_else(|| runtime_err!("cross: expected 2 vectors"))?)
                .ok_or_else(|| runtime_err!("cross: second arg must be a list of numbers"))?;
            if a.len() != 3 || b.len() != 3 {
                return Err(runtime_err!("cross: both vectors must have exactly 3 elements"));
            }
            let result = vec![
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            from_vec(result)
        }

        "norm" => {
            let a = to_vec(args.first().ok_or_else(|| runtime_err!("norm: expected a vector"))?)
                .ok_or_else(|| runtime_err!("norm: arg must be a list of numbers"))?;
            let result: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
            Value::Float(result)
        }

        "normalize" => {
            let a = to_vec(args.first().ok_or_else(|| runtime_err!("normalize: expected a vector"))?)
                .ok_or_else(|| runtime_err!("normalize: arg must be a list of numbers"))?;
            let n: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
            if n == 0.0 {
                return Err(runtime_err!("normalize: cannot normalize a zero vector"));
            }
            from_vec(a.iter().map(|x| x / n).collect())
        }

        "vec_add" => {
            let a = to_vec(args.first().ok_or_else(|| runtime_err!("vec_add: expected 2 vectors"))?)
                .ok_or_else(|| runtime_err!("vec_add: first arg must be a list of numbers"))?;
            let b = to_vec(args.get(1).ok_or_else(|| runtime_err!("vec_add: expected 2 vectors"))?)
                .ok_or_else(|| runtime_err!("vec_add: second arg must be a list of numbers"))?;
            if a.len() != b.len() {
                return Err(runtime_err!("vec_add: vectors must have the same length ({} vs {})", a.len(), b.len()));
            }
            from_vec(a.iter().zip(b.iter()).map(|(x, y)| x + y).collect())
        }

        "vec_sub" => {
            let a = to_vec(args.first().ok_or_else(|| runtime_err!("vec_sub: expected 2 vectors"))?)
                .ok_or_else(|| runtime_err!("vec_sub: first arg must be a list of numbers"))?;
            let b = to_vec(args.get(1).ok_or_else(|| runtime_err!("vec_sub: expected 2 vectors"))?)
                .ok_or_else(|| runtime_err!("vec_sub: second arg must be a list of numbers"))?;
            if a.len() != b.len() {
                return Err(runtime_err!("vec_sub: vectors must have the same length ({} vs {})", a.len(), b.len()));
            }
            from_vec(a.iter().zip(b.iter()).map(|(x, y)| x - y).collect())
        }

        "vec_scale" => {
            let a = to_vec(args.first().ok_or_else(|| runtime_err!("vec_scale: expected vector and scalar"))?)
                .ok_or_else(|| runtime_err!("vec_scale: first arg must be a list of numbers"))?;
            let s = args.get(1)
                .and_then(|v| v.as_float())
                .ok_or_else(|| runtime_err!("vec_scale: second arg must be a number"))?;
            from_vec(a.iter().map(|x| x * s).collect())
        }

        // ── Matrix operations ───────────────────────────────────────────────
        "mat_add" => {
            let a = to_matrix(args.first().ok_or_else(|| runtime_err!("mat_add: expected 2 matrices"))?)
                .ok_or_else(|| runtime_err!("mat_add: first arg must be a matrix (list of lists)"))?;
            let b = to_matrix(args.get(1).ok_or_else(|| runtime_err!("mat_add: expected 2 matrices"))?)
                .ok_or_else(|| runtime_err!("mat_add: second arg must be a matrix (list of lists)"))?;
            if a.len() != b.len() {
                return Err(runtime_err!("mat_add: matrices must have the same dimensions"));
            }
            let mut result = Vec::with_capacity(a.len());
            for (row_a, row_b) in a.iter().zip(b.iter()) {
                if row_a.len() != row_b.len() {
                    return Err(runtime_err!("mat_add: matrices must have the same dimensions"));
                }
                result.push(row_a.iter().zip(row_b.iter()).map(|(x, y)| x + y).collect());
            }
            from_matrix(result)
        }

        "mat_sub" => {
            let a = to_matrix(args.first().ok_or_else(|| runtime_err!("mat_sub: expected 2 matrices"))?)
                .ok_or_else(|| runtime_err!("mat_sub: first arg must be a matrix (list of lists)"))?;
            let b = to_matrix(args.get(1).ok_or_else(|| runtime_err!("mat_sub: expected 2 matrices"))?)
                .ok_or_else(|| runtime_err!("mat_sub: second arg must be a matrix (list of lists)"))?;
            if a.len() != b.len() {
                return Err(runtime_err!("mat_sub: matrices must have the same dimensions"));
            }
            let mut result = Vec::with_capacity(a.len());
            for (row_a, row_b) in a.iter().zip(b.iter()) {
                if row_a.len() != row_b.len() {
                    return Err(runtime_err!("mat_sub: matrices must have the same dimensions"));
                }
                result.push(row_a.iter().zip(row_b.iter()).map(|(x, y)| x - y).collect());
            }
            from_matrix(result)
        }

        "mat_mul" => {
            let a = to_matrix(args.first().ok_or_else(|| runtime_err!("mat_mul: expected 2 matrices"))?)
                .ok_or_else(|| runtime_err!("mat_mul: first arg must be a matrix"))?;
            let b = to_matrix(args.get(1).ok_or_else(|| runtime_err!("mat_mul: expected 2 matrices"))?)
                .ok_or_else(|| runtime_err!("mat_mul: second arg must be a matrix"))?;
            if a.is_empty() || b.is_empty() {
                return Err(runtime_err!("mat_mul: matrices must not be empty"));
            }
            let cols_a = a[0].len();
            let rows_b = b.len();
            if cols_a != rows_b {
                return Err(runtime_err!("mat_mul: incompatible dimensions ({}x{} * {}x{})",
                    a.len(), cols_a, rows_b, b[0].len()));
            }
            let cols_b = b[0].len();
            let mut result = vec![vec![0.0; cols_b]; a.len()];
            for i in 0..a.len() {
                for j in 0..cols_b {
                    let mut sum = 0.0;
                    for k in 0..cols_a {
                        sum += a[i][k] * b[k][j];
                    }
                    result[i][j] = sum;
                }
            }
            from_matrix(result)
        }

        "mat_scale" => {
            let m = to_matrix(args.first().ok_or_else(|| runtime_err!("mat_scale: expected matrix and scalar"))?)
                .ok_or_else(|| runtime_err!("mat_scale: first arg must be a matrix"))?;
            let s = args.get(1)
                .and_then(|v| v.as_float())
                .ok_or_else(|| runtime_err!("mat_scale: second arg must be a number"))?;
            let result: Vec<Vec<f64>> = m.iter()
                .map(|row| row.iter().map(|x| x * s).collect())
                .collect();
            from_matrix(result)
        }

        "transpose" => {
            let m = to_matrix(args.first().ok_or_else(|| runtime_err!("transpose: expected a matrix"))?)
                .ok_or_else(|| runtime_err!("transpose: arg must be a matrix"))?;
            if m.is_empty() {
                from_matrix(vec![])
            } else {
                let rows = m.len();
                let cols = m[0].len();
                let mut result = vec![vec![0.0; rows]; cols];
                for i in 0..rows {
                    for j in 0..cols {
                        result[j][i] = m[i][j];
                    }
                }
                from_matrix(result)
            }
        }

        "trace" => {
            let m = to_matrix(args.first().ok_or_else(|| runtime_err!("trace: expected a matrix"))?)
                .ok_or_else(|| runtime_err!("trace: arg must be a matrix"))?;
            if m.is_empty() {
                Value::Float(0.0)
            } else {
                let n = m.len().min(m[0].len());
                let result: f64 = (0..n).map(|i| m[i][i]).sum();
                Value::Float(result)
            }
        }

        "identity" => {
            let n = args.first()
                .and_then(|v| v.as_int())
                .ok_or_else(|| runtime_err!("identity: expected integer size"))? as usize;
            let mut result = vec![vec![0.0; n]; n];
            for i in 0..n {
                result[i][i] = 1.0;
            }
            from_matrix(result)
        }

        "zeros" => {
            let rows = args.first()
                .and_then(|v| v.as_int())
                .ok_or_else(|| runtime_err!("zeros: expected integer rows"))? as usize;
            let cols = args.get(1)
                .and_then(|v| v.as_int())
                .ok_or_else(|| runtime_err!("zeros: expected integer cols"))? as usize;
            from_matrix(vec![vec![0.0; cols]; rows])
        }

        "det" => {
            let m = to_matrix(args.first().ok_or_else(|| runtime_err!("det: expected a matrix"))?)
                .ok_or_else(|| runtime_err!("det: arg must be a square matrix"))?;
            if m.is_empty() {
                Value::Float(1.0)
            } else {
                let n = m.len();
                if m.iter().any(|row| row.len() != n) {
                    return Err(runtime_err!("det: matrix must be square"));
                }
                Value::Float(determinant(&m))
            }
        }

        "inv" => {
            let m = to_matrix(args.first().ok_or_else(|| runtime_err!("inv: expected a matrix"))?)
                .ok_or_else(|| runtime_err!("inv: arg must be a square matrix"))?;
            let n = m.len();
            if n == 0 || m.iter().any(|row| row.len() != n) {
                return Err(runtime_err!("inv: matrix must be non-empty and square"));
            }
            match gauss_jordan_inverse(&m) {
                Some(result) => from_matrix(result),
                None => return Err(runtime_err!("inv: matrix is singular (non-invertible)")),
            }
        }

        "solve" => {
            let a = to_matrix(args.first().ok_or_else(|| runtime_err!("solve: expected matrix A and vector b"))?)
                .ok_or_else(|| runtime_err!("solve: first arg must be a matrix"))?;
            let b = to_vec(args.get(1).ok_or_else(|| runtime_err!("solve: expected matrix A and vector b"))?)
                .ok_or_else(|| runtime_err!("solve: second arg must be a vector"))?;
            let n = a.len();
            if n == 0 || a.iter().any(|row| row.len() != n) {
                return Err(runtime_err!("solve: matrix A must be non-empty and square"));
            }
            if b.len() != n {
                return Err(runtime_err!("solve: vector b length must match matrix dimension"));
            }
            match gaussian_elimination(&a, &b) {
                Some(result) => from_vec(result),
                None => return Err(runtime_err!("solve: system has no unique solution")),
            }
        }

        "eigenvalues" => {
            let m = to_matrix(args.first().ok_or_else(|| runtime_err!("eigenvalues: expected a matrix"))?)
                .ok_or_else(|| runtime_err!("eigenvalues: arg must be a 2x2 matrix"))?;
            if m.len() != 2 || m[0].len() != 2 || m[1].len() != 2 {
                return Err(runtime_err!("eigenvalues: only 2x2 matrices are supported"));
            }
            let a = m[0][0];
            let b = m[0][1];
            let c = m[1][0];
            let d = m[1][1];
            // Characteristic equation: λ² - (a+d)λ + (ad-bc) = 0
            let trace = a + d;
            let det = a * d - b * c;
            let discriminant = trace * trace - 4.0 * det;
            if discriminant >= 0.0 {
                let sq = discriminant.sqrt();
                let l1 = (trace + sq) / 2.0;
                let l2 = (trace - sq) / 2.0;
                from_vec(vec![l1, l2])
            } else {
                // Complex eigenvalues — return [real_part, imag_part] pairs
                let real = trace / 2.0;
                let imag = (-discriminant).sqrt() / 2.0;
                Value::make_list(vec![
                    from_vec(vec![real, imag]),
                    from_vec(vec![real, -imag]),
                ])
            }
        }

        "rank" => {
            let m = to_matrix(args.first().ok_or_else(|| runtime_err!("rank: expected a matrix"))?)
                .ok_or_else(|| runtime_err!("rank: arg must be a matrix"))?;
            if m.is_empty() {
                Value::Int(0)
            } else {
                Value::Int(matrix_rank(&m) as i64)
            }
        }

        // ── Unknown — let the caller decide ─────────────────────────────────
        _ => return Ok(None),
    };
    Ok(Some(v))
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal algorithms
// ─────────────────────────────────────────────────────────────────────────────

/// Recursive determinant computation (cofactor expansion for small matrices,
/// LU-style for larger ones via row reduction).
fn determinant(m: &[Vec<f64>]) -> f64 {
    let n = m.len();
    if n == 0 { return 1.0; }
    if n == 1 { return m[0][0]; }
    if n == 2 { return m[0][0] * m[1][1] - m[0][1] * m[1][0]; }
    if n == 3 {
        return m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
             - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
             + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    }

    // For larger matrices, use row reduction with partial pivoting
    let mut a: Vec<Vec<f64>> = m.to_vec();
    let mut det = 1.0;
    for col in 0..n {
        // Find pivot
        let mut max_row = col;
        let mut max_val = a[col][col].abs();
        for row in (col + 1)..n {
            if a[row][col].abs() > max_val {
                max_val = a[row][col].abs();
                max_row = row;
            }
        }
        if max_val < 1e-12 {
            return 0.0;
        }
        if max_row != col {
            a.swap(col, max_row);
            det = -det;
        }
        det *= a[col][col];
        let pivot = a[col][col];
        for row in (col + 1)..n {
            let factor = a[row][col] / pivot;
            for j in col..n {
                a[row][j] -= factor * a[col][j];
            }
        }
    }
    det
}

/// Gauss-Jordan elimination to compute matrix inverse.
fn gauss_jordan_inverse(m: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = m.len();
    // Build augmented matrix [m | I]
    let mut aug: Vec<Vec<f64>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = Vec::with_capacity(2 * n);
        row.extend_from_slice(&m[i]);
        for j in 0..n {
            row.push(if i == j { 1.0 } else { 0.0 });
        }
        aug.push(row);
    }

    for col in 0..n {
        // Partial pivoting
        let mut max_row = col;
        let mut max_val = aug[col][col].abs();
        for row in (col + 1)..n {
            if aug[row][col].abs() > max_val {
                max_val = aug[row][col].abs();
                max_row = row;
            }
        }
        if max_val < 1e-12 {
            return None; // Singular
        }
        if max_row != col {
            aug.swap(col, max_row);
        }

        // Scale pivot row
        let pivot = aug[col][col];
        for j in 0..(2 * n) {
            aug[col][j] /= pivot;
        }

        // Eliminate column in all other rows
        for row in 0..n {
            if row == col { continue; }
            let factor = aug[row][col];
            for j in 0..(2 * n) {
                aug[row][j] -= factor * aug[col][j];
            }
        }
    }

    // Extract the right half
    let result: Vec<Vec<f64>> = aug.iter()
        .map(|row| row[n..].to_vec())
        .collect();
    Some(result)
}

/// Gaussian elimination with partial pivoting to solve Ax = b.
fn gaussian_elimination(a: &[Vec<f64>], b: &[f64]) -> Option<Vec<f64>> {
    let n = a.len();
    // Build augmented matrix [A | b]
    let mut aug: Vec<Vec<f64>> = Vec::with_capacity(n);
    for i in 0..n {
        let mut row = Vec::with_capacity(n + 1);
        row.extend_from_slice(&a[i]);
        row.push(b[i]);
        aug.push(row);
    }

    // Forward elimination
    for col in 0..n {
        // Partial pivoting
        let mut max_row = col;
        let mut max_val = aug[col][col].abs();
        for row in (col + 1)..n {
            if aug[row][col].abs() > max_val {
                max_val = aug[row][col].abs();
                max_row = row;
            }
        }
        if max_val < 1e-12 {
            return None; // Singular or no unique solution
        }
        if max_row != col {
            aug.swap(col, max_row);
        }
        let pivot = aug[col][col];
        for row in (col + 1)..n {
            let factor = aug[row][col] / pivot;
            for j in col..=n {
                aug[row][j] -= factor * aug[col][j];
            }
        }
    }

    // Back substitution
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut sum = aug[i][n];
        for j in (i + 1)..n {
            sum -= aug[i][j] * x[j];
        }
        x[i] = sum / aug[i][i];
    }
    Some(x)
}

/// Matrix rank via row echelon form.
fn matrix_rank(m: &[Vec<f64>]) -> usize {
    let rows = m.len();
    if rows == 0 { return 0; }
    let cols = m[0].len();
    let mut a: Vec<Vec<f64>> = m.to_vec();
    let mut rank = 0;
    let mut pivot_col = 0;

    for row in 0..rows {
        if pivot_col >= cols { break; }
        // Find pivot in current column
        let mut found = false;
        for r in row..rows {
            if a[r][pivot_col].abs() > 1e-12 {
                a.swap(row, r);
                found = true;
                break;
            }
        }
        if !found {
            pivot_col += 1;
            // Retry this row with next column
            if pivot_col < cols {
                // We need to re-check this row index, so decrement won't work
                // in a for loop. Use a workaround: just continue scanning.
                // Actually, let's redo with a while loop approach below.
            }
            continue;
        }
        // Eliminate below
        let pivot = a[row][pivot_col];
        for r in (row + 1)..rows {
            let factor = a[r][pivot_col] / pivot;
            for c in pivot_col..cols {
                a[r][c] -= factor * a[row][c];
            }
        }
        rank += 1;
        pivot_col += 1;
    }
    rank
}
