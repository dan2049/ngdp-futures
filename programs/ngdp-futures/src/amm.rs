//! Pool pricing: a constant-product market maker over the two sides of a contract
//! (the "fixed product market maker" design used by Gnosis/Omen prediction markets).
//!
//! The pool holds `up` units (pay more when NGDP lands higher) and `down` units
//! (pay more when it lands lower). One up + one down = one fully collateralised set.
//! Trading never lets `up * down` fall, so the pool can't be drained and every
//! price stays strictly inside the band.
//!
//! Marginal price of the up side, as a fraction of a set = down / (up + down).
//! Implied gap ($bn) = floor + (cap - floor) * down / (up + down).
//!
//! Units: 1 contract = 1_000_000 units. Rounding always favours the pool.

/// Integer square root, rounded down.
pub fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = (n as f64).sqrt() as u128;
    while x.checked_mul(x).map_or(true, |v| v > n) {
        x -= 1;
    }
    while (x + 1).checked_mul(x + 1).map_or(false, |v| v <= n) {
        x += 1;
    }
    x
}

/// Buy exactly `q` units of one side from the pool.
/// `out_bal` = pool balance of the side being bought, `other_bal` = the other side.
/// Returns (sets_minted, new_out_bal, new_other_bal). The buyer pays for `sets_minted` sets.
pub fn buy_exact(out_bal: u128, other_bal: u128, q: u128) -> Option<(u128, u128, u128)> {
    if out_bal == 0 || other_bal == 0 || q == 0 {
        return None;
    }
    let k = out_bal.checked_mul(other_bal)?;
    // Solve (out + n - q)(other + n) = out * other for n:
    //   n^2 + n(out + other - q) - q*other = 0
    let s: i128 = out_bal as i128 + other_bal as i128 - q as i128;
    let disc: u128 = (s.checked_mul(s)? as u128).checked_add(q.checked_mul(other_bal)?.checked_mul(4)?)?;
    let root = isqrt(disc) as i128;
    let mut n: u128 = ((root - s).max(0) as u128) / 2;
    // Round up until the invariant holds (at most a couple of steps).
    loop {
        if out_bal + n > q {
            let lhs = (out_bal + n - q).checked_mul(other_bal + n)?;
            if lhs >= k {
                break;
            }
        }
        n += 1;
    }
    Some((n, out_bal + n - q, other_bal + n))
}

/// Sell exactly `q` units of one side back to the pool.
/// `in_bal` = pool balance of the side being returned, `other_bal` = the other side.
/// Returns (sets_burned, new_in_bal, new_other_bal). The seller receives `sets_burned` sets of collateral.
pub fn sell_exact(in_bal: u128, other_bal: u128, q: u128) -> Option<(u128, u128, u128)> {
    if in_bal == 0 || other_bal == 0 || q == 0 {
        return None;
    }
    let k = in_bal.checked_mul(other_bal)?;
    // Solve (in + q - n)(other - n) = in * other for the smaller root n:
    //   n^2 - n(in + q + other) + q*other = 0
    let t = in_bal.checked_add(q)?.checked_add(other_bal)?;
    let disc = t.checked_mul(t)?.checked_sub(q.checked_mul(other_bal)?.checked_mul(4)?)?;
    let mut root = isqrt(disc);
    if root * root < disc {
        root += 1; // ceil
    }
    let mut n = t.checked_sub(root)? / 2;
    if n >= other_bal {
        n = other_bal - 1;
    }
    // Round down until the invariant holds.
    while n > 0 && (in_bal + q - n).checked_mul(other_bal - n)? < k {
        n -= 1;
    }
    Some((n, in_bal + q - n, other_bal - n))
}

#[cfg(test)]
mod tests {
    use super::*;
    const C: u128 = 1_000_000; // one contract

    #[test]
    fn sqrt_works() {
        for n in [0u128, 1, 2, 3, 4, 15, 16, 17, 1_000_000, 10u128.pow(30) + 7] {
            let r = isqrt(n);
            assert!(r * r <= n && (r + 1) * (r + 1) > n, "{}", n);
        }
    }

    #[test]
    fn even_pool_prices_half() {
        // 50 contracts each side; buying a tiny amount costs ~half a set.
        let (n, _, _) = buy_exact(50 * C, 50 * C, 1_000).unwrap();
        assert!(n >= 500 && n <= 502, "n = {}", n);
    }

    #[test]
    fn buying_one_contract_from_100k_pool() {
        // $100,000 pool at $2,000/set = 50 contracts each side.
        let (n, up, down) = buy_exact(50 * C, 50 * C, C).unwrap();
        let cost_usd = n as f64 * 2000.0 / 1e6; // sets * $2,000 per contract-set / 1e6 units
        assert!(cost_usd > 1000.0 && cost_usd < 1025.0, "cost {}", cost_usd);
        assert!(up * down >= 2500 * C * C);
        // price moved up: down/(up+down) > 0.5
        assert!(down * 2 > up + down);
    }

    #[test]
    fn round_trip_never_profits() {
        let (a0, b0) = (37 * C + 123, 61 * C + 7);
        for q in [1u128, 999, C, 7 * C, 30 * C] {
            let (paid, a1, b1) = buy_exact(a0, b0, q).unwrap();
            let (got, a2, b2) = sell_exact(a1, b1, q).unwrap();
            assert!(got <= paid, "q {}: paid {} got {}", q, paid, got);
            assert!(a2 * b2 >= a0 * b0);
        }
    }

    #[test]
    fn invariant_never_falls_and_prices_stay_inside() {
        let (mut a, mut b) = (50 * C, 50 * C);
        let k0 = a * b;
        for i in 0..200u128 {
            let q = (i * 7919) % (20 * C) + 1;
            if i % 3 == 0 {
                let (_, na, nb) = buy_exact(a, b, q).unwrap(); a = na; b = nb;
            } else if i % 3 == 1 {
                let (_, nb, na) = buy_exact(b, a, q).unwrap(); a = na; b = nb;
            } else {
                let (_, na, nb) = sell_exact(a, b, q).unwrap(); a = na; b = nb;
            }
            assert!(a * b >= k0 && a > 0 && b > 0);
        }
    }

    #[test]
    fn huge_buy_gets_expensive_but_works() {
        let (n, up, _) = buy_exact(50 * C, 50 * C, 500 * C).unwrap();
        let avg_price = n as f64 / (500.0 * C as f64); // fraction of a set per contract
        assert!(avg_price > 0.9 && avg_price < 1.0, "{}", avg_price);
        assert!(up > 0);
    }

    #[test]
    fn sell_more_than_pool_can_pay_is_capped() {
        // Returning a lot can never pay out the pool's last unit of the other side.
        let (n, _, other) = sell_exact(10 * C, 10 * C, 10_000 * C).unwrap();
        assert!(n < 10 * C && other >= 1);
    }
}
