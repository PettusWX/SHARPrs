//! Low-level numeric helpers that reproduce the exact floating-point
//! behaviour of the NumPy primitives SHARPpy is built on.
//!
//! Internally the crate represents "missing" as `NaN`; these helpers follow
//! NumPy masked-array semantics on top of that (masked entries contribute `0`
//! to sums, are excluded from counts, etc.). None of this is public API.

/// `pow` from the platform libm, *without* letting LLVM rewrite
/// `pow(x, 2.0)` into `x * x` (NumPy calls libm `pow` for scalar `x**2`
/// and `np.power(x, 2)`, and the two differ in the last bit ~0.1 % of
/// the time).
#[inline]
pub(crate) fn pow(x: f64, y: f64) -> f64 {
    x.powf(std::hint::black_box(y))
}

/// `np.power(x, 2)` / scalar `x**2`.
#[inline]
pub(crate) fn sq(x: f64) -> f64 {
    pow(x, 2.0)
}

/// NumPy's pairwise summation (`np.add.reduce` on a contiguous float64
/// array). Bit-for-bit identical to `np.sum` for 1-D arrays.
pub(crate) fn pairwise_sum(a: &[f64]) -> f64 {
    let n = a.len();
    if n < 8 {
        let mut res = 0.;
        for &x in a {
            res += x;
        }
        res
    } else if n <= 128 {
        let mut r = [a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7]];
        let mut i = 8;
        while i < n - (n % 8) {
            for j in 0..8 {
                r[j] += a[i + j];
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        while i < n {
            res += a[i];
            i += 1;
        }
        res
    } else {
        let mut n2 = n / 2;
        n2 -= n2 % 8;
        pairwise_sum(&a[..n2]) + pairwise_sum(&a[n2..])
    }
}

/// [`pairwise_sum`] of the virtual array `f(off)..f(off + n - 1)` without
/// materialising it: the same summation tree, so the same bits.
#[inline]
pub(crate) fn pairwise_sum_by<F: Fn(usize) -> f64>(off: usize, n: usize, f: &F) -> f64 {
    if n < 8 {
        let mut res = 0.;
        for i in 0..n {
            res += f(off + i);
        }
        res
    } else if n <= 128 {
        let mut r = [f(off), f(off + 1), f(off + 2), f(off + 3), f(off + 4), f(off + 5), f(off + 6), f(off + 7)];
        let mut i = 8;
        while i < n - (n % 8) {
            for j in 0..8 {
                r[j] += f(off + i + j);
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        while i < n {
            res += f(off + i);
            i += 1;
        }
        res
    } else {
        let mut n2 = n / 2;
        n2 -= n2 % 8;
        pairwise_sum_by(off, n2, f) + pairwise_sum_by(off + n2, n - n2, f)
    }
}

/// Number of elements [`arange`] produces (0 where it returns empty).
#[inline]
pub(crate) fn arange_len(start: f64, stop: f64, step: f64) -> usize {
    if start.is_nan() || stop.is_nan() || step.is_nan() || step == 0. {
        return 0;
    }
    let len = ((stop - start) / step).ceil();
    if len.is_nan() || len <= 0. {
        return 0;
    }
    len as usize
}

/// `masked_average(a, w)` for two arrays `a`, `b` that are masked at the
/// same positions (u/v wind components), sharing the weight sum. Identical
/// to calling [`masked_average`] twice.
pub(crate) fn masked_average2(a: &[f64], b: &[f64], w: &[f64]) -> (f64, f64) {
    debug_assert!(a.len() == w.len() && b.len() == w.len());
    let n = w.len();
    let ra = if a.iter().all(|x| x.is_nan()) {
        None
    } else {
        Some(pairwise_sum_by(0, n, &|i| if a[i].is_nan() { 0. } else { a[i] * w[i] }))
    };
    let rb = if b.iter().all(|x| x.is_nan()) {
        None
    } else {
        Some(pairwise_sum_by(0, n, &|i| if b[i].is_nan() { 0. } else { b[i] * w[i] }))
    };
    if ra.is_none() && rb.is_none() {
        return (f64::NAN, f64::NAN);
    }
    let same_mask = a.iter().zip(b).all(|(x, y)| x.is_nan() == y.is_nan());
    let wa = pairwise_sum_by(0, n, &|i| if a[i].is_nan() { 0. } else { w[i] });
    let wb = if same_mask { wa } else { pairwise_sum_by(0, n, &|i| if b[i].is_nan() { 0. } else { w[i] }) };
    (ra.map_or(f64::NAN, |s| s / wa), rb.map_or(f64::NAN, |s| s / wb))
}

/// Sum of a masked array: masked (NaN) entries are filled with zero first,
/// exactly like `MaskedArray.sum()`. Returns NaN when every entry is masked
/// (NumPy returns `masked`).
pub(crate) fn masked_sum(a: &[f64]) -> f64 {
    if a.iter().all(|x| x.is_nan()) {
        return f64::NAN;
    }
    pairwise_sum_by(0, a.len(), &|i| if a[i].is_nan() { 0. } else { a[i] })
}

/// Sum of the entries selected by `pred` (`x[x > 0].sum()` idiom).
pub(crate) fn select_sum(a: &[f64], pred: impl Fn(f64) -> bool) -> f64 {
    let sel: Vec<f64> = a.iter().copied().filter(|&x| !x.is_nan() && pred(x)).collect();
    pairwise_sum(&sel)
}

/// `MaskedArray.mean()`.
pub(crate) fn masked_mean(a: &[f64]) -> f64 {
    let cnt = a.iter().filter(|x| !x.is_nan()).count();
    if cnt == 0 {
        return f64::NAN;
    }
    masked_sum(a) * 1. / cnt as f64
}

/// `np.ma.average(a, weights=w)`: weights of masked entries are zeroed.
pub(crate) fn masked_average(a: &[f64], w: &[f64]) -> f64 {
    debug_assert_eq!(a.len(), w.len());
    if a.iter().all(|x| x.is_nan()) {
        return f64::NAN;
    }
    let sum = pairwise_sum_by(0, a.len(), &|i| if a[i].is_nan() { 0. } else { a[i] * w[i] });
    let weight = pairwise_sum_by(0, a.len(), &|i| if a[i].is_nan() { 0. } else { w[i] });
    sum / weight
}

/// `np.arange(start, stop, step)` for float64, including NumPy's way of
/// generating elements (`start + i * ((start + step) - start)`).
pub(crate) fn arange(start: f64, stop: f64, step: f64) -> Vec<f64> {
    if start.is_nan() || stop.is_nan() || step.is_nan() || step == 0. {
        return Vec::new();
    }
    let len = ((stop - start) / step).ceil();
    if len.is_nan() || len <= 0. {
        return Vec::new();
    }
    let len = len as usize;
    let mut out = Vec::with_capacity(len);
    out.push(start);
    if len == 1 {
        return out;
    }
    let second = start + step;
    out.push(second);
    let delta = second - start;
    for i in 2..len {
        out.push(start + (i as f64) * delta);
    }
    out
}

/// NumPy 1.23 `binary_search_with_guess`.
fn binary_search_with_guess(key: f64, arr: &[f64], guess: isize) -> isize {
    const LIKELY_IN_CACHE_SIZE: isize = 8;
    let len = arr.len() as isize;
    let mut imin: isize = 0;
    let mut imax: isize = len;
    if key > arr[(len - 1) as usize] {
        return len;
    } else if key < arr[0] {
        return -1;
    }
    if len <= 4 {
        // linear_search(key, arr, len, 1)
        let mut i = 1;
        while i < len && key >= arr[i as usize] {
            i += 1;
        }
        return i - 1;
    }
    let mut guess = guess;
    if guess > len - 3 {
        guess = len - 3;
    }
    if guess < 1 {
        guess = 1;
    }
    let a = |i: isize| arr[i as usize];
    if key < a(guess) {
        if key < a(guess - 1) {
            imax = guess - 1;
            if guess > LIKELY_IN_CACHE_SIZE && key >= a(guess - LIKELY_IN_CACHE_SIZE) {
                imin = guess - LIKELY_IN_CACHE_SIZE;
            }
        } else {
            return guess - 1;
        }
    } else if key < a(guess + 1) {
        return guess;
    } else if key < a(guess + 2) {
        return guess + 1;
    } else {
        imin = guess + 2;
        if guess < len - LIKELY_IN_CACHE_SIZE - 1 && key < a(guess + LIKELY_IN_CACHE_SIZE) {
            imax = guess + LIKELY_IN_CACHE_SIZE;
        }
    }
    while imin < imax {
        let imid = imin + ((imax - imin) >> 1);
        if key >= a(imid) {
            imin = imid + 1;
        } else {
            imax = imid;
        }
    }
    imin - 1
}

/// One evaluation of `np.interp` (NumPy 1.23 algorithm) with explicit
/// out-of-range fill values. `guess` carries the search hint across a
/// vectorised call exactly like NumPy does.
#[inline]
fn interp_one(x: f64, xp: &[f64], fp: &[f64], left: f64, right: f64, guess: &mut isize) -> f64 {
    let n = xp.len();
    if n == 0 {
        return f64::NAN;
    }
    if x.is_nan() {
        return x;
    }
    if n == 1 {
        return if x < xp[0] {
            left
        } else if x > xp[0] {
            right
        } else {
            fp[0]
        };
    }
    let j = binary_search_with_guess(x, xp, *guess);
    *guess = j;
    interp_at(j, x, xp, fp, left, right)
}

/// The interpolation step of [`interp_one`] given the search result `j`.
#[inline]
fn interp_at(j: isize, x: f64, xp: &[f64], fp: &[f64], left: f64, right: f64) -> f64 {
    let n = xp.len();
    if j == -1 {
        left
    } else if j == n as isize {
        right
    } else {
        let j = j as usize;
        if j == n - 1 || xp[j] == x {
            fp[j]
        } else {
            let slope = (fp[j + 1] - fp[j]) / (xp[j + 1] - xp[j]);
            let mut r = slope * (x - xp[j]) + fp[j];
            if r.is_nan() {
                r = slope * (x - xp[j + 1]) + fp[j + 1];
                if r.is_nan() && fp[j] == fp[j + 1] {
                    r = fp[j];
                }
            }
            r
        }
    }
}

/// [`interp_many`] for `N` tables sharing the same `xp` (one search per
/// point; the search result depends only on `x`, `xp` and the guess, so
/// this equals `N` separate vectorised calls).
pub(crate) fn interp_many_n<const N: usize>(xs: &[f64], xp: &[f64], fps: [&[f64]; N]) -> [Vec<f64>; N] {
    let n = xp.len();
    let mut out: [Vec<f64>; N] = std::array::from_fn(|_| Vec::with_capacity(xs.len()));
    if n == 0 {
        for o in out.iter_mut() {
            o.resize(xs.len(), f64::NAN);
        }
        return out;
    }
    let mut g = 0;
    for &x in xs {
        if x.is_nan() {
            for o in out.iter_mut() {
                o.push(x);
            }
        } else if n == 1 {
            for (o, fp) in out.iter_mut().zip(fps) {
                o.push(if x < xp[0] { f64::NAN } else if x > xp[0] { f64::NAN } else { fp[0] });
            }
        } else {
            let j = binary_search_with_guess(x, xp, g);
            g = j;
            for (o, fp) in out.iter_mut().zip(fps) {
                o.push(interp_at(j, x, xp, fp, f64::NAN, f64::NAN));
            }
        }
    }
    out
}

/// `np.interp(x, xp, fp, left=nan, right=nan)` for a scalar.
#[inline]
pub(crate) fn interp(x: f64, xp: &[f64], fp: &[f64]) -> f64 {
    let mut g = 0;
    interp_one(x, xp, fp, f64::NAN, f64::NAN, &mut g)
}

/// `np.interp(x, xp, fp)` with NumPy's default edge fill (`fp[0]`,
/// `fp[-1]`).
pub(crate) fn interp_clamped(x: f64, xp: &[f64], fp: &[f64]) -> f64 {
    let mut g = 0;
    let (l, r) = (fp[0], fp[fp.len() - 1]);
    interp_one(x, xp, fp, l, r, &mut g)
}

/// Vectorised `np.interp(xs, xp, fp, left=nan, right=nan)`.
pub(crate) fn interp_many(xs: &[f64], xp: &[f64], fp: &[f64]) -> Vec<f64> {
    let mut g = 0;
    xs.iter()
        .map(|&x| interp_one(x, xp, fp, f64::NAN, f64::NAN, &mut g))
        .collect()
}

/// NumPy `np.isclose(a, b)` with default tolerances.
#[inline]
pub(crate) fn isclose(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-8 + 1e-5 * b.abs()
}

/// `np.ma.argmax`: masked (NaN) entries filled with -inf; first maximum.
pub(crate) fn masked_argmax(a: &[f64]) -> Option<usize> {
    if a.is_empty() {
        return None;
    }
    let mut best = 0;
    let mut bv = f64::NEG_INFINITY;
    for (i, &x) in a.iter().enumerate() {
        let x = if x.is_nan() { f64::NEG_INFINITY } else { x };
        if x > bv {
            bv = x;
            best = i;
        }
    }
    Some(best)
}

/// `np.ma.argmin`: masked (NaN) entries filled with +inf; first minimum.
pub(crate) fn masked_argmin(a: &[f64]) -> Option<usize> {
    if a.is_empty() {
        return None;
    }
    let mut best = 0;
    let mut bv = f64::INFINITY;
    for (i, &x) in a.iter().enumerate() {
        let x = if x.is_nan() { f64::INFINITY } else { x };
        if x < bv {
            bv = x;
            best = i;
        }
    }
    Some(best)
}

/// NumPy 1.23 `aquicksort` for float64 (introsort; insertion sort for
/// small partitions). Reproduced because SHARPpy's `max_wind` depends on
/// how ties are ordered by this (unstable) sort.
pub(crate) fn np_argsort(v: &[f64]) -> Vec<usize> {
    let num = v.len();
    let mut tosort: Vec<usize> = (0..num).collect();
    if num < 2 {
        return tosort;
    }
    #[inline]
    fn lt(a: f64, b: f64) -> bool {
        a < b || (b.is_nan() && !a.is_nan())
    }
    const SMALL_QUICKSORT: isize = 15;
    let mut stack: Vec<(isize, isize, i32)> = Vec::new();
    let mut pl: isize = 0;
    let mut pr: isize = num as isize - 1;
    let msb = (usize::BITS - 1 - num.leading_zeros()) as i32;
    let mut cdepth: i32 = msb * 2;
    let t = &mut tosort;
    loop {
        if cdepth < 0 {
            // heapsort fallback (practically unreachable for sounding sizes)
            let slice = &mut t[pl as usize..=pr as usize];
            slice.sort_by(|&a, &b| v[a].partial_cmp(&v[b]).unwrap_or(std::cmp::Ordering::Equal));
        } else {
            while (pr - pl) > SMALL_QUICKSORT {
                let pm = pl + ((pr - pl) >> 1);
                if lt(v[t[pm as usize]], v[t[pl as usize]]) {
                    t.swap(pm as usize, pl as usize);
                }
                if lt(v[t[pr as usize]], v[t[pm as usize]]) {
                    t.swap(pr as usize, pm as usize);
                }
                if lt(v[t[pm as usize]], v[t[pl as usize]]) {
                    t.swap(pm as usize, pl as usize);
                }
                let vp = v[t[pm as usize]];
                let mut pi = pl;
                let mut pj = pr - 1;
                t.swap(pm as usize, pj as usize);
                loop {
                    loop {
                        pi += 1;
                        if !lt(v[t[pi as usize]], vp) {
                            break;
                        }
                    }
                    loop {
                        pj -= 1;
                        if !lt(vp, v[t[pj as usize]]) {
                            break;
                        }
                    }
                    if pi >= pj {
                        break;
                    }
                    t.swap(pi as usize, pj as usize);
                }
                let pk = pr - 1;
                t.swap(pi as usize, pk as usize);
                cdepth -= 1;
                if pi - pl < pr - pi {
                    stack.push((pi + 1, pr, cdepth));
                    pr = pi - 1;
                } else {
                    stack.push((pl, pi - 1, cdepth));
                    pl = pi + 1;
                }
            }
            // insertion sort
            let mut pi = pl + 1;
            while pi <= pr {
                let vi = t[pi as usize];
                let vpv = v[vi];
                let mut pj = pi;
                let mut pk = pi - 1;
                while pj > pl && lt(vpv, v[t[pk as usize]]) {
                    t[pj as usize] = t[pk as usize];
                    pj -= 1;
                    pk -= 1;
                }
                t[pj as usize] = vi;
                pi += 1;
            }
        }
        match stack.pop() {
            None => break,
            Some((l, r, d)) => {
                pl = l;
                pr = r;
                cdepth = d;
            }
        }
    }
    tosort
}
