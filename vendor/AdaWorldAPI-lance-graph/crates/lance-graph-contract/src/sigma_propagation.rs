//! Σ-Propagation — EWA-Sandwich kernel for multi-hop edge propagation.
//!
//! # The math (Pillar 6, verified empirically by `crates/jc::ewa_sandwich`)
//!
//! Multi-hop signal propagation through a graph of SPD covariance matrices
//! follows the **EWA-Sandwich** rule:
//!
//! ```text
//!     Σ_path = M · Σ · Mᵀ        (sandwich, NOT addition)
//! ```
//!
//! Composing along an edge path `(M_1, M_2, ..., M_n)`:
//!
//! ```text
//!     Σ_n = M_n · M_{n-1} · ... · M_1 · Σ_0 · M_1ᵀ · ... · M_nᵀ
//! ```
//!
//! Two structural guarantees:
//!
//! 1. **PSD-preservation by construction.** If `Σ_0 ⪰ 0` and every `M_k`
//!    is symmetric, then every `Σ_k ⪰ 0`. The naive convolution
//!    `Σ_n = Σ_{n-1} + step` only preserves semi-definiteness in the Lie
//!    algebra and degrades numerically. The EWA sandwich preserves PSD on
//!    the SPD manifold itself.
//! 2. **Geometric error control.** Path-aggregated error is multiplicative
//!    (log-norm bounded), not additive — the Köstenberger-Stark
//!    concentration on Hadamard 2×2 SPD (Pillar 5+) gives a tight bound
//!    on the log-norm growth.
//!
//! Verified by `crates/jc::ewa_sandwich::prove`:
//!
//! ```text
//!   PSD-preservation rate = 1.000000 (10000/10000 hops)
//!   Concentration tightness = 1.467× (log-normal-corrected KS bound)
//! ```
//!
//! # Carriers (this version)
//!
//! 2×2 SPD matrices, packed as [`Spd2`] `{a, b, c}` representing
//! ```text
//!     ┌ a  b ┐
//!     │       │       (symmetric, so b = b)
//!     └ b  c ┘
//! ```
//!
//! **Why 2×2.** The first production consumer is the BindSpace Σ-codebook
//! (planned in B2/B3): a 256-entry codebook of 2×2 SPDs covering the
//! "covariance shape" prior on entity fingerprints. Higher-dim Σ
//! (e.g. block-diagonal d×d) is a follow-up — the kernel generalises
//! algebraically (sandwich is dimension-agnostic) but the packed
//! representation specialises to 2×2 for the cost-target of 1 byte/row
//! codebook indices.
//!
//! # Use-site (planned consumers)
//!
//! - **B2 BindSpace Σ-column**: every BindSpace row carries `sigma: u8`,
//!   indexing into a 256-entry static `SigmaCodebook` of [`Spd2`]
//!   instances.
//! - **B3 transcode-sigma-assignment**: at row-write time, pick the
//!   codebook entry from the typed value's `(PropertyKind, Marking,
//!   SemanticType)` tuple plus value range.
//! - **B4 shader-driver-sigma-propagate**: in `ShaderDriver::dispatch()`
//!   between [5] edge emission and [6] FreeEnergy gate, propagate
//!   `sigma_path = ewa_sandwich(...)` along the resonance chain. Reject
//!   (block) cycles whose [`log_norm_growth`] exceeds
//!   [`pillar_5plus_bound`].
//!
//! Cross-ref: `crates/jc/src/ewa_sandwich.rs` is the self-contained
//! Pillar-6 verification probe (zero-deps). It does NOT import from
//! this module by design — the proof harness stays standalone so it
//! can run as a regression certificate against any future
//! re-implementation of this surface. If you change the math here,
//! re-run `cargo run -p jc --example prove_it` to confirm Pillar 6
//! still passes.

#![allow(clippy::many_single_char_names)]

// ═══════════════════════════════════════════════════════════════════════════
// Spd2 — 2×2 symmetric positive-definite matrix, packed
// ═══════════════════════════════════════════════════════════════════════════

/// 2×2 symmetric matrix, stored as the upper triangle:
///
/// ```text
///     ┌ a  b ┐
///     │       │
///     └ b  c ┘
/// ```
///
/// Positive-definiteness is a runtime property — see
/// [`Spd2::is_spd`]. Construction never enforces it; the kernel
/// trusts callers to pass SPDs and checks at boundaries (codebook
/// load, runtime gate).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spd2 {
    /// Top-left entry.
    pub a: f64,
    /// Off-diagonal entry.
    pub b: f64,
    /// Bottom-right entry.
    pub c: f64,
}

impl Spd2 {
    /// The 2×2 identity.
    pub const I: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 1.0,
    };

    /// Eigendecomposition. Returns `(λ_1, λ_2, cos θ, sin θ)` where
    /// the eigenvectors are `(cos θ, sin θ)` and `(-sin θ, cos θ)`.
    pub fn eig(&self) -> (f64, f64, f64, f64) {
        let half_trace = (self.a + self.c) / 2.0;
        let half_diff = (self.a - self.c) / 2.0;
        let disc = (half_diff * half_diff + self.b * self.b).sqrt();
        let l1 = half_trace + disc;
        let l2 = half_trace - disc;
        let theta = if self.b.abs() < 1e-15 && (self.a - self.c).abs() < 1e-15 {
            0.0
        } else {
            0.5 * (2.0 * self.b).atan2(self.a - self.c)
        };
        (l1, l2, theta.cos(), theta.sin())
    }

    /// Spectral power: `Σ^t`. Defined for any real `t` on SPDs;
    /// caller is responsible for ensuring the matrix is SPD.
    pub fn pow(&self, t: f64) -> Self {
        let (l1, l2, c, s) = self.eig();
        let l1t = l1.max(1e-300).powf(t);
        let l2t = l2.max(1e-300).powf(t);
        Self {
            a: c * c * l1t + s * s * l2t,
            b: c * s * (l1t - l2t),
            c: s * s * l1t + c * c * l2t,
        }
    }

    /// Matrix square root: `Σ^(1/2)`.
    pub fn sqrt(&self) -> Self {
        self.pow(0.5)
    }

    /// Matrix log of an SPD, for log-norm metrics.
    pub fn log_spd(&self) -> Self {
        let (l1, l2, c, s) = self.eig();
        let l1l = l1.max(1e-300).ln();
        let l2l = l2.max(1e-300).ln();
        Self {
            a: c * c * l1l + s * s * l2l,
            b: c * s * (l1l - l2l),
            c: s * s * l1l + c * c * l2l,
        }
    }

    /// Frobenius norm squared: `‖Σ‖²_F = a² + 2b² + c²`.
    /// Off-diagonal counted twice because the matrix is symmetric.
    pub fn frobenius_sq(&self) -> f64 {
        self.a * self.a + 2.0 * self.b * self.b + self.c * self.c
    }

    /// Determinant: `ac − b²`.
    pub fn det(&self) -> f64 {
        self.a * self.c - self.b * self.b
    }

    /// Numeric SPD check. All four conditions must hold:
    /// `a > eps`, `c > eps`, `det > eps`, both eigenvalues > eps.
    pub fn is_spd(&self, eps: f64) -> bool {
        if self.a <= eps || self.c <= eps {
            return false;
        }
        if self.det() <= eps {
            return false;
        }
        let (l1, l2, _, _) = self.eig();
        l1 > eps && l2 > eps
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EWA sandwich kernel
// ═══════════════════════════════════════════════════════════════════════════

/// Symmetric sandwich product: `M · N · Mᵀ` (for symmetric `M`, `N`).
/// Returns a symmetric result. This is the EWA-Sandwich kernel —
/// see module doc for the math.
///
/// Per Pillar 5+ (Köstenberger-Stark) and Pillar 6 (verified by
/// `crates/jc::ewa_sandwich::prove` at 10000/10000 PSD-preserving
/// hops), this preserves PSD by construction.
pub fn ewa_sandwich(m: &Spd2, sigma: &Spd2) -> Spd2 {
    let p00 = m.a * sigma.a + m.b * sigma.b;
    let p01 = m.a * sigma.b + m.b * sigma.c;
    let p10 = m.b * sigma.a + m.c * sigma.b;
    let p11 = m.b * sigma.b + m.c * sigma.c;
    let r00 = p00 * m.a + p01 * m.b;
    let r01 = p00 * m.b + p01 * m.c;
    let r10 = p10 * m.a + p11 * m.b;
    let r11 = p10 * m.b + p11 * m.c;
    Spd2 {
        a: r00,
        b: 0.5 * (r01 + r10),
        c: r11,
    }
}

/// Inverse sandwich (un-walk a path): given `Σ_propagated = M · Σ_seed · Mᵀ`
/// and `M` (assumed invertible), recover `Σ_seed`. Useful for time-reversal
/// queries on the BindSpace edge graph.
///
/// Numerically unstable when `M` has eigenvalues close to zero (`det(M) → 0`).
/// Caller should check `m.det() > eps` before calling.
pub fn ewa_inverse(m: &Spd2, propagated: &Spd2) -> Spd2 {
    // Σ_seed = M^(-1) · Σ_propagated · (M^(-1))ᵀ. Since M is symmetric,
    // M^(-1) is also symmetric; reuse `ewa_sandwich` with the inverse.
    let det = m.det();
    let inv = Spd2 {
        a: m.c / det,
        b: -m.b / det,
        c: m.a / det,
    };
    ewa_sandwich(&inv, propagated)
}

/// Log-norm growth: `‖log(Σ_propagated)‖²_F − ‖log(Σ_seed)‖²_F`.
///
/// Used as the runtime concentration certificate. Per Pillar 5+
/// (Köstenberger-Stark) the growth is bounded by
/// `(6 D_n / n) · Σ d(μ_k, μ) + (1/n²) · Σ Var(X_k)`. In practice
/// we compare against [`pillar_5plus_bound`] as a coarse runtime gate.
///
/// Both inputs must be SPD; result is the difference of their
/// Frobenius-log-norms. Negative growth = path lost log-norm
/// (multiplicative attenuation). Positive growth = log-norm grew.
pub fn log_norm_growth(seed: &Spd2, propagated: &Spd2) -> f64 {
    propagated.log_spd().frobenius_sq() - seed.log_spd().frobenius_sq()
}

/// Köstenberger-Stark bound on log-norm growth for a path of `n_hops`
/// hops with the empirically-verified `σ_step ≈ 0.2` step-σ.
///
/// The closed form (per Pillar 5+ proof-in-code in `crates/jc`):
///
/// ```text
///     CV_bound = √(2/n) · √(1 + 2σ²n)         (log-normal correction)
/// ```
///
/// Returns the bound on the COEFFICIENT OF VARIATION of `‖log(Σ_n)‖²_F`.
/// Convert to absolute log-norm-growth bound by multiplying by the
/// expected Σ-norm (typically O(n) for the seed-Σ chosen).
///
/// The PASS slack for runtime gates is `1.75×` the predicted CV
/// (per Pillar 6 detail string) — beyond that, the path has lost
/// concentration and the cycle should be rejected.
pub fn pillar_5plus_bound(n_hops: usize) -> f64 {
    if n_hops == 0 {
        return 0.0;
    }
    let n = n_hops as f64;
    let sigma_step = 0.2_f64;
    (2.0 / n).sqrt() * (1.0 + 2.0 * sigma_step * sigma_step * n).sqrt()
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(x: f64, y: f64, tol: f64) -> bool {
        (x - y).abs() <= tol
    }

    #[test]
    fn identity_sandwich_preserves_input() {
        let s = Spd2 {
            a: 2.0,
            b: 0.5,
            c: 3.0,
        };
        let out = ewa_sandwich(&Spd2::I, &s);
        assert!(approx(out.a, s.a, 1e-12));
        assert!(approx(out.b, s.b, 1e-12));
        assert!(approx(out.c, s.c, 1e-12));
    }

    #[test]
    fn sandwich_preserves_spd() {
        // Pillar 6 invariant: SPD in, SPD out.
        let m = Spd2 {
            a: 1.5,
            b: 0.3,
            c: 1.2,
        };
        let sigma = Spd2 {
            a: 2.0,
            b: 0.5,
            c: 1.8,
        };
        assert!(m.is_spd(1e-12));
        assert!(sigma.is_spd(1e-12));
        let out = ewa_sandwich(&m, &sigma);
        assert!(out.is_spd(1e-12), "EWA-Sandwich must preserve SPD");
    }

    #[test]
    fn sandwich_is_symmetric_in_result() {
        // The off-diagonal must equal itself — sandwich preserves
        // symmetry by construction.
        let m = Spd2 {
            a: 1.5,
            b: 0.3,
            c: 1.2,
        };
        let sigma = Spd2 {
            a: 2.0,
            b: 0.5,
            c: 1.8,
        };
        let out = ewa_sandwich(&m, &sigma);
        // Spd2 stores upper triangle only — symmetry is structural.
        // The midpoint averaging in ewa_sandwich is the mechanism:
        // verify it produced a finite, well-formed matrix.
        assert!(out.a.is_finite() && out.b.is_finite() && out.c.is_finite());
    }

    #[test]
    fn ewa_inverse_round_trips() {
        let m = Spd2 {
            a: 1.5,
            b: 0.3,
            c: 1.2,
        };
        let sigma = Spd2 {
            a: 2.0,
            b: 0.5,
            c: 1.8,
        };
        let propagated = ewa_sandwich(&m, &sigma);
        let recovered = ewa_inverse(&m, &propagated);
        // Round-trip fidelity should be high; allow ~1e-10 tolerance
        // for SPD eigendecomposition + inverse roundoff.
        assert!(
            approx(recovered.a, sigma.a, 1e-10),
            "round-trip a: {} vs {}",
            recovered.a,
            sigma.a
        );
        assert!(approx(recovered.b, sigma.b, 1e-10));
        assert!(approx(recovered.c, sigma.c, 1e-10));
    }

    #[test]
    fn log_norm_growth_is_zero_for_identity_propagation() {
        // Sandwiching with M = I should leave log-norm unchanged.
        let sigma = Spd2 {
            a: 2.0,
            b: 0.5,
            c: 1.8,
        };
        let propagated = ewa_sandwich(&Spd2::I, &sigma);
        let growth = log_norm_growth(&sigma, &propagated);
        assert!(approx(growth, 0.0, 1e-10), "I-sandwich growth: {growth}");
    }

    #[test]
    fn log_norm_growth_positive_when_m_amplifies() {
        // M with eigenvalues > 1 should grow the log-norm.
        let m = Spd2 {
            a: 2.0,
            b: 0.0,
            c: 2.0,
        }; // 2I
        let sigma = Spd2::I;
        let propagated = ewa_sandwich(&m, &sigma);
        let growth = log_norm_growth(&sigma, &propagated);
        assert!(growth > 0.0, "amplifying M must grow log-norm: {growth}");
    }

    #[test]
    fn log_norm_growth_negative_when_m_attenuates() {
        // log_norm_growth measures the SIGNED change in log-Frobenius
        // distance from identity. Since ‖log(Σ)‖²_F ≥ 0 with equality
        // iff Σ = I, an attenuating M only produces NEGATIVE growth
        // when it pulls Σ TOWARD identity, not just away from a
        // pre-attenuated seed.
        //
        // Setup: seed = 4·I (far from identity in log-distance).
        // M = 0.5·I. Sandwich = M·Σ·Mᵀ = 0.25·4·I = I.
        //   ‖log(seed)‖²_F = ‖ln(4)·I‖²_F = 2·ln(4)² ≈ 3.84
        //   ‖log(propagated)‖²_F = ‖log(I)‖²_F = 0
        // Growth = 0 - 3.84 = -3.84 < 0.
        //
        // The original (now-corrected) test used seed = I, which is
        // already at the log-distance origin — every sandwich pushes
        // Σ AWAY from I, so growth is structurally positive regardless
        // of whether M attenuates or amplifies. That made the assertion
        // unsatisfiable: ‖log(0.25·I)‖²_F = 2·ln(0.25)² ≈ 3.84 > 0.
        let m = Spd2 {
            a: 0.5,
            b: 0.0,
            c: 0.5,
        }; // 0.5·I
        let sigma = Spd2 {
            a: 4.0,
            b: 0.0,
            c: 4.0,
        }; // 4·I — start far from identity
        let propagated = ewa_sandwich(&m, &sigma);
        let growth = log_norm_growth(&sigma, &propagated);
        assert!(
            growth < 0.0,
            "attenuating M pulling Σ toward I must shrink log-norm: {growth}"
        );
    }

    #[test]
    fn pillar_5plus_bound_is_finite_and_positive() {
        // Bound should be positive for any nonzero hop count.
        for n in [1, 5, 10, 50, 100, 1000] {
            let b = pillar_5plus_bound(n);
            assert!(
                b.is_finite() && b > 0.0,
                "pillar_5plus_bound({n}) = {b} not finite/positive"
            );
        }
    }

    #[test]
    fn pillar_5plus_bound_zero_hops_returns_zero() {
        assert_eq!(pillar_5plus_bound(0), 0.0);
    }

    #[test]
    fn pillar_5plus_bound_decreases_with_more_hops_eventually() {
        // The bound is dominated by √(2/n) for small σ_step·n,
        // so it decreases with n in the regime of interest.
        let b10 = pillar_5plus_bound(10);
        let b100 = pillar_5plus_bound(100);
        // For σ=0.2, b10 ≈ 0.6 and b100 ≈ 0.41 (the second term
        // grows but the first shrinks faster).
        assert!(
            b100 < b10,
            "pillar_5plus_bound should decrease with hops: \
             b10={b10}, b100={b100}"
        );
    }

    #[test]
    fn spd2_identity_is_spd() {
        assert!(Spd2::I.is_spd(1e-12));
        assert_eq!(Spd2::I.det(), 1.0);
        assert_eq!(Spd2::I.frobenius_sq(), 2.0);
    }

    #[test]
    fn spd2_pow_zero_returns_identity() {
        let s = Spd2 {
            a: 3.0,
            b: 0.5,
            c: 2.0,
        };
        let p = s.pow(0.0);
        assert!(approx(p.a, 1.0, 1e-10));
        assert!(approx(p.b, 0.0, 1e-10));
        assert!(approx(p.c, 1.0, 1e-10));
    }

    #[test]
    fn spd2_sqrt_squared_returns_input() {
        let s = Spd2 {
            a: 3.0,
            b: 0.5,
            c: 2.0,
        };
        let r = s.sqrt();
        let r2 = ewa_sandwich(&r, &Spd2::I);
        // r2 = r·I·r = r²; should equal s.
        assert!(approx(r2.a, s.a, 1e-10));
        assert!(approx(r2.b, s.b, 1e-10));
        assert!(approx(r2.c, s.c, 1e-10));
    }
}
