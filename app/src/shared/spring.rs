//! Springs described the way Apple does: a duration (how long until it looks
//! finished) and a bounce (0 = no overshoot). Values come from
//! docs/brand/BRAND.md. A spring keeps its velocity when the target changes,
//! so a state change mid-motion turns around instead of restarting.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringSpec {
    pub duration: f32,
    pub bounce: f32,
}

/// Pill state changes, toggles.
#[allow(dead_code, reason = "used by the pill in phase 2")]
pub const FAST: SpringSpec = SpringSpec { duration: 0.25, bounce: 0.0 };
/// Windows and onboarding steps.
pub const DEFAULT: SpringSpec = SpringSpec { duration: 0.4, bounce: 0.0 };
/// Onboarding finish and the first successful dictation. Nothing else.
#[allow(dead_code, reason = "used by the onboarding finish in phase 3")]
pub const PLAYFUL: SpringSpec = SpringSpec { duration: 0.5, bounce: 0.3 };

#[derive(Debug, Clone, Copy)]
pub struct Spring {
    stiffness: f32,
    damping: f32,
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
}

impl Spring {
    pub fn new(spec: SpringSpec, value: f32) -> Self {
        // Mass 1: stiffness = (2π / duration)², damping = 4π(1 − bounce) / duration.
        let tau = std::f32::consts::TAU;
        Self {
            stiffness: (tau / spec.duration).powi(2),
            damping: 2.0 * tau * (1.0 - spec.bounce) / spec.duration,
            value,
            velocity: 0.0,
            target: value,
        }
    }

    /// Advances by `dt` seconds. Returns true once it has come to rest, so the
    /// caller can stop requesting frames.
    pub fn step(&mut self, dt: f32) -> bool {
        // Small fixed substeps keep stiff springs stable at low frame rates.
        let steps = (dt / 0.004).ceil().max(1.0) as u32;
        let h = dt / steps as f32;
        for _ in 0..steps {
            let accel = -self.stiffness * (self.value - self.target) - self.damping * self.velocity;
            self.velocity += accel * h;
            self.value += self.velocity * h;
        }
        let settled = self.is_settled();
        if settled {
            self.value = self.target;
            self.velocity = 0.0;
        }
        settled
    }

    pub fn is_settled(&self) -> bool {
        (self.value - self.target).abs() < 0.01 && self.velocity.abs() < 0.01
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(spring: &mut Spring, seconds: f32) -> (f32, bool) {
        let mut peak = spring.value;
        let mut settled = false;
        for _ in 0..(seconds * 120.0) as usize {
            settled = spring.step(1.0 / 120.0);
            peak = peak.max(spring.value);
        }
        (peak, settled)
    }

    #[test]
    fn bounce_zero_reaches_target_without_overshoot() {
        let mut s = Spring::new(FAST, 0.0);
        s.target = 100.0;
        let (peak, settled) = run(&mut s, 1.0);
        assert!(settled);
        assert_eq!(s.value, 100.0);
        assert!(peak <= 100.0 + 0.01, "overshot to {peak}");
    }

    #[test]
    fn playful_overshoots_a_little() {
        let mut s = Spring::new(PLAYFUL, 0.0);
        s.target = 100.0;
        let (peak, _) = run(&mut s, 2.0);
        assert!(peak > 102.0 && peak < 110.0, "peak {peak}");
    }

    #[test]
    fn retargeting_keeps_velocity() {
        let mut s = Spring::new(DEFAULT, 0.0);
        s.target = 100.0;
        for _ in 0..12 {
            s.step(1.0 / 120.0);
        }
        let moving = s.velocity;
        assert!(moving > 0.0);
        s.target = 0.0;
        s.step(1.0 / 120.0);
        assert!(s.velocity > 0.0, "should still be carrying momentum, got {}", s.velocity);
    }
}
