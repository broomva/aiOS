//! Recursive Controlled Systems — formal control-theoretic types.
//!
//! Defines the RCS 7-tuple `Σ = (X, Y, U, f, h, S, Π)` as Rust traits,
//! parameterized by hierarchy level. The controller `Π` at each level is
//! itself an RCS at the next level, producing a self-similar hierarchy.
//!
//! # Levels
//!
//! | Level | Name | Controller |
//! |-------|------|------------|
//! | L0 | External plant | Arcan agent loop |
//! | L1 | Agent internal | Autonomic rule engine |
//! | L2 | Meta-control | EGRI proposer + selector |
//! | L3 | Governance | bstack policy rules |
//!
//! # References
//!
//! - Eslami & Yu (2026), arXiv:2603.10779 — stability budget, CBF-QP shields
//! - Keramati & Gutkin (2014) — homeostatic drive as Lyapunov function
//! - Ashby (1952) — requisite variety, ultrastability

use std::fmt;

// ---------------------------------------------------------------------------
// Level markers
// ---------------------------------------------------------------------------

/// Marker trait for RCS hierarchy levels.
///
/// Each level is a zero-sized type used as a generic parameter to
/// [`RecursiveControlledSystem`], [`LyapunovCandidate`], and related traits.
pub trait Level: Send + Sync + 'static + fmt::Debug {
    /// Human-readable name (e.g., "L0: External Plant").
    fn name() -> &'static str;

    /// Numeric index (0–3).
    fn index() -> usize;
}

/// Level 0 — External plant (microgrid, codebase, conversation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct L0;

/// Level 1 — Agent internal (homeostatic regulation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct L1;

/// Level 2 — Meta-control (EGRI recursive improvement).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct L2;

/// Level 3 — Governance (bstack policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct L3;

impl Level for L0 {
    fn name() -> &'static str {
        "L0: External Plant"
    }
    fn index() -> usize {
        0
    }
}
impl Level for L1 {
    fn name() -> &'static str {
        "L1: Agent Internal"
    }
    fn index() -> usize {
        1
    }
}
impl Level for L2 {
    fn name() -> &'static str {
        "L2: Meta-Control (EGRI)"
    }
    fn index() -> usize {
        2
    }
}
impl Level for L3 {
    fn name() -> &'static str {
        "L3: Governance"
    }
    fn index() -> usize {
        3
    }
}

// ---------------------------------------------------------------------------
// Core RCS trait
// ---------------------------------------------------------------------------

/// The RCS 7-tuple as a Rust trait, parameterized by hierarchy level.
///
/// ```text
/// Σ = (X, Y, U, f, h, S, Π)
///
/// X = Self::State          — state space
/// Y = Self::Observation    — observation space
/// U = Self::Control        — control input space
/// f = step()               — dynamics: X × U → X
/// h = observe()            — observation map: X → Y
/// S = shield()             — safety shield: U × X → U
/// Π = (the next level)     — controller is an RCS at L+1
/// ```
pub trait RecursiveControlledSystem<L: Level> {
    /// X — the system's internal state.
    type State: Send + Sync;

    /// Y — what can be measured from the state.
    type Observation: Send + Sync;

    /// U — what the controller can change.
    type Control: Send + Sync;

    /// h: X → Y — observe the current state.
    fn observe(&self) -> Self::Observation;

    /// f: X × U → X — compute next state given a control input.
    fn step(&mut self, control: &Self::Control);

    /// S: U × X → U — filter a proposed control input to a safe one.
    ///
    /// The default implementation is the identity (no filtering).
    /// Override to implement CBF-QP, hysteresis gates, or budget checks.
    fn shield(&self, proposed: Self::Control) -> Self::Control {
        proposed
    }
}

// ---------------------------------------------------------------------------
// Lyapunov candidate
// ---------------------------------------------------------------------------

/// A candidate Lyapunov function for stability analysis at a given level.
///
/// The homeostatic drive `D(x) = ‖x − x*‖²` is the canonical candidate
/// at Level 1. It simultaneously serves as:
/// 1. Lyapunov function (stability)
/// 2. Reward signal (RL)
/// 3. Free energy bound (active inference)
pub trait LyapunovCandidate<L: Level> {
    /// The state type this function operates on.
    type State;

    /// V(x) — evaluate the Lyapunov function at state `x`.
    ///
    /// Must satisfy `V(x) ≥ 0` with `V(x*) = 0`.
    fn evaluate(&self, state: &Self::State) -> f64;

    /// ΔV estimate — negative means the system is converging.
    ///
    /// Approximates `V(x_{k+1}) − V(x_k)`. A controller that makes
    /// this consistently negative is stabilizing.
    fn decrease_rate(&self, state: &Self::State) -> f64;

    /// ν — jump bound at switching instants.
    ///
    /// At each mode switch: `V(x⁺) ≤ ν · V(x⁻)` where `ν ≥ 1`.
    /// Used in the switching cost term `(ln ν) / τ_a` of the
    /// stability budget.
    fn jump_bound(&self) -> f64 {
        1.0 // No jump growth by default.
    }
}

// ---------------------------------------------------------------------------
// Stability budget
// ---------------------------------------------------------------------------

/// The recursive stability budget: `λ = γ − Σ costs > 0`.
///
/// For the composite system to be exponentially stable, `λ > 0` must hold
/// at every level simultaneously (Theorem 1, RCS definitions).
///
/// ```text
/// λ = γ − L_θ·ρ − L_d·η − β·τ̄ − (ln ν)/τ_a
/// ```
#[derive(Debug, Clone, Copy)]
pub struct StabilityBudget {
    /// γ — nominal decay rate of the Lyapunov function.
    ///
    /// How fast V(x) decreases when all higher levels are frozen.
    pub decay_rate: f64,

    /// L_θ·ρ — adaptation cost from the level above tuning parameters.
    pub adaptation_cost: f64,

    /// L_d·η — design evolution cost from the level above changing architecture.
    pub design_cost: f64,

    /// β·τ̄ — delay cost from inference, tool, and communication latency.
    pub delay_cost: f64,

    /// (ln ν)/τ_a — switching cost from mode transitions.
    pub switching_cost: f64,
}

impl StabilityBudget {
    /// λ = γ − Σ costs.
    pub fn margin(&self) -> f64 {
        self.decay_rate
            - self.adaptation_cost
            - self.design_cost
            - self.delay_cost
            - self.switching_cost
    }

    /// Is the system stable at this level? (`λ > 0`)
    pub fn is_stable(&self) -> bool {
        self.margin() > 0.0
    }

    /// Returns a diagnostic breakdown of the budget terms.
    pub fn breakdown(&self) -> StabilityBreakdown {
        StabilityBreakdown {
            margin: self.margin(),
            decay_rate: self.decay_rate,
            adaptation_cost: self.adaptation_cost,
            design_cost: self.design_cost,
            delay_cost: self.delay_cost,
            switching_cost: self.switching_cost,
            is_stable: self.is_stable(),
        }
    }
}

impl fmt::Display for StabilityBudget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = self.margin();
        let status = if m > 0.0 { "STABLE" } else { "UNSTABLE" };
        write!(
            f,
            "λ = {:.4} [{status}]  (γ={:.3} − adapt={:.3} − design={:.3} − delay={:.3} − switch={:.3})",
            m, self.decay_rate, self.adaptation_cost, self.design_cost, self.delay_cost, self.switching_cost
        )
    }
}

/// Detailed breakdown of a stability budget evaluation.
#[derive(Debug, Clone, Copy)]
pub struct StabilityBreakdown {
    /// λ — the net stability margin.
    pub margin: f64,
    /// γ — nominal decay rate.
    pub decay_rate: f64,
    /// L_θ·ρ — adaptation cost.
    pub adaptation_cost: f64,
    /// L_d·η — design cost.
    pub design_cost: f64,
    /// β·τ̄ — delay cost.
    pub delay_cost: f64,
    /// (ln ν)/τ_a — switching cost.
    pub switching_cost: f64,
    /// Whether λ > 0.
    pub is_stable: bool,
}

// ---------------------------------------------------------------------------
// Composite stability
// ---------------------------------------------------------------------------

/// Checks recursive stability across all levels of an RCS hierarchy.
///
/// Returns `true` iff `λᵢ > 0` for all provided budgets.
/// The minimum margin determines the composite decay rate.
pub fn is_recursively_stable(budgets: &[StabilityBudget]) -> bool {
    budgets.iter().all(|b| b.is_stable())
}

/// Returns the minimum stability margin across all levels.
///
/// The composite system decays at rate `ω = min_i λᵢ`.
/// Returns `None` if the slice is empty.
pub fn composite_margin(budgets: &[StabilityBudget]) -> Option<f64> {
    budgets.iter().map(|b| b.margin()).reduce(f64::min)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_markers() {
        assert_eq!(L0::index(), 0);
        assert_eq!(L1::index(), 1);
        assert_eq!(L2::index(), 2);
        assert_eq!(L3::index(), 3);
        assert_eq!(L0::name(), "L0: External Plant");
        assert_eq!(L2::name(), "L2: Meta-Control (EGRI)");
    }

    #[test]
    fn stability_budget_stable() {
        let budget = StabilityBudget {
            decay_rate: 1.0,
            adaptation_cost: 0.2,
            design_cost: 0.1,
            delay_cost: 0.15,
            switching_cost: 0.05,
        };
        assert!(budget.is_stable());
        assert!((budget.margin() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn stability_budget_unstable() {
        let budget = StabilityBudget {
            decay_rate: 0.3,
            adaptation_cost: 0.2,
            design_cost: 0.1,
            delay_cost: 0.15,
            switching_cost: 0.05,
        };
        assert!(!budget.is_stable());
        assert!(budget.margin() < 0.0);
    }

    #[test]
    fn stability_budget_display() {
        let budget = StabilityBudget {
            decay_rate: 1.0,
            adaptation_cost: 0.2,
            design_cost: 0.1,
            delay_cost: 0.15,
            switching_cost: 0.05,
        };
        let s = format!("{budget}");
        assert!(s.contains("STABLE"));
        assert!(s.contains("0.5000"));
    }

    #[test]
    fn recursive_stability_all_stable() {
        let budgets = vec![
            StabilityBudget {
                decay_rate: 1.0,
                adaptation_cost: 0.1,
                design_cost: 0.1,
                delay_cost: 0.1,
                switching_cost: 0.1,
            },
            StabilityBudget {
                decay_rate: 0.5,
                adaptation_cost: 0.05,
                design_cost: 0.05,
                delay_cost: 0.05,
                switching_cost: 0.05,
            },
        ];
        assert!(is_recursively_stable(&budgets));
        assert!((composite_margin(&budgets).unwrap() - 0.3).abs() < 1e-10);
    }

    #[test]
    fn recursive_stability_one_unstable() {
        let budgets = vec![
            StabilityBudget {
                decay_rate: 1.0,
                adaptation_cost: 0.1,
                design_cost: 0.1,
                delay_cost: 0.1,
                switching_cost: 0.1,
            },
            StabilityBudget {
                decay_rate: 0.1,
                adaptation_cost: 0.1,
                design_cost: 0.1,
                delay_cost: 0.1,
                switching_cost: 0.1,
            },
        ];
        assert!(!is_recursively_stable(&budgets));
        assert!(composite_margin(&budgets).unwrap() < 0.0);
    }

    #[test]
    fn composite_margin_empty() {
        assert!(composite_margin(&[]).is_none());
    }

    #[test]
    fn breakdown_matches_budget() {
        let budget = StabilityBudget {
            decay_rate: 0.8,
            adaptation_cost: 0.15,
            design_cost: 0.1,
            delay_cost: 0.2,
            switching_cost: 0.05,
        };
        let bd = budget.breakdown();
        assert_eq!(bd.margin, budget.margin());
        assert_eq!(bd.is_stable, budget.is_stable());
        assert_eq!(bd.decay_rate, budget.decay_rate);
    }

    // Minimal dummy implementation to verify trait compiles.
    struct DummyPlant {
        state: f64,
    }

    impl RecursiveControlledSystem<L0> for DummyPlant {
        type State = f64;
        type Observation = f64;
        type Control = f64;

        fn observe(&self) -> f64 {
            self.state
        }

        fn step(&mut self, control: &f64) {
            self.state += control;
        }

        fn shield(&self, proposed: f64) -> f64 {
            proposed.clamp(-1.0, 1.0) // Simple saturation shield.
        }
    }

    impl LyapunovCandidate<L0> for DummyPlant {
        type State = f64;

        fn evaluate(&self, state: &f64) -> f64 {
            state * state // V(x) = x²
        }

        fn decrease_rate(&self, state: &f64) -> f64 {
            // Dummy: assumes control drives state toward 0.
            -2.0 * state.abs()
        }
    }

    #[test]
    fn dummy_plant_trait_usage() {
        let mut plant = DummyPlant { state: 5.0 };
        assert_eq!(plant.observe(), 5.0);

        // Shield clamps control to [-1, 1].
        let safe = plant.shield(10.0);
        assert_eq!(safe, 1.0);

        plant.step(&-0.5);
        assert_eq!(plant.observe(), 4.5);

        // Lyapunov: V(4.5) = 20.25.
        let v = LyapunovCandidate::<L0>::evaluate(&plant, &4.5);
        assert!((v - 20.25).abs() < 1e-10);
    }
}
