//! Springs, which every animation moves by, so that a change of target in
//! the middle of a motion carries on from where it is, at the speed it has,
//! instead of jumping or starting over.

/// A damped spring of unit mass, stepped by real time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
    stiffness: f32,
    damping: f32,
}

impl Spring {
    pub fn new(value: f32) -> Self {
        Self {
            value,
            velocity: 0.0,
            target: value,
            stiffness: 300.0,
            damping: 24.0,
        }
    }

    /// `ratio` is the damping ratio: 1 settles without overshoot, below 1
    /// bounces.
    pub fn aim(&mut self, target: f32, stiffness: f32, ratio: f32) {
        self.target = target;
        self.stiffness = stiffness;
        self.damping = 2.0 * ratio * stiffness.sqrt();
    }

    /// Semi-implicit Euler in steps of at most 1 ms, which stays stable for
    /// any stiffness an animation uses. A step longer than 0.1 s, such as
    /// after the compositor stopped asking for frames while the surface was
    /// hidden, counts as 0.1 s, so the motion resumes rather than jumps.
    pub fn step(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        let n = (dt / 0.001).ceil().max(1.0) as usize;
        let h = dt / n as f32;
        for _ in 0..n {
            let a = -self.stiffness * (self.value - self.target) - self.damping * self.velocity;
            self.velocity += a * h;
            self.value += self.velocity * h;
        }
        if self.settled() {
            self.value = self.target;
            self.velocity = 0.0;
        }
    }

    /// Closer than anything a frame shows: 5e-4 of a 300-pixel motion is a
    /// sixth of a pixel. Far from 0 that is finer than an f32 resolves: at
    /// 637, a step of 1 ms at 0.03 pixels a second rounds back to where it
    /// was, and a spring there would stop short of its target and never
    /// count as settled, so the margins grow with the target.
    pub fn settled(&self) -> bool {
        let far = self.target.abs();
        (self.value - self.target).abs() < 5e-4 + 1e-5 * far
            && self.velocity.abs() < 5e-3 + 1e-4 * far
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &mut Spring, secs: f32) -> Vec<f32> {
        (0..(secs * 60.0) as usize)
            .map(|_| {
                s.step(1.0 / 60.0);
                s.value
            })
            .collect()
    }

    /// Where a card that came to rest 637 pixels out stuck, drawing for
    /// good, at 43 frames a second.
    #[test]
    fn far_from_0_it_still_settles() {
        let mut s = Spring::new(637.0017);
        s.velocity = -0.029_186_5;
        s.aim(637.0, 420.0, 0.6);
        for _ in 0..43 {
            s.step(1.0 / 43.0);
        }
        assert!(s.settled(), "{s:?}");
    }

    #[test]
    fn opening_bounces_a_little_and_settles() {
        let mut s = Spring::new(0.0);
        s.aim(1.0, 260.0, 0.68);
        let v = run(&mut s, 1.0);
        let peak = v.iter().copied().fold(0.0, f32::max);
        assert!(peak > 1.02 && peak < 1.08, "overshoot {peak}");
        assert!(s.settled());
    }

    #[test]
    fn closing_never_passes_its_target() {
        let mut s = Spring::new(1.0);
        s.aim(0.0, 420.0, 1.0);
        let v = run(&mut s, 0.6);
        assert!(v.iter().all(|&x| x >= -1e-3), "{v:?}");
        assert!(s.settled());
    }

    #[test]
    fn a_new_target_keeps_the_speed_it_has() {
        let mut s = Spring::new(0.0);
        s.aim(1.0, 260.0, 0.68);
        s.step(0.05);
        let speed = s.velocity;
        s.aim(0.0, 420.0, 1.0);
        assert_eq!(s.velocity, speed);
        s.step(0.001);
        assert!(s.value > 0.0);
    }

    #[test]
    fn a_long_pause_counts_as_a_tenth_of_a_second() {
        let (mut a, mut b) = (Spring::new(0.0), Spring::new(0.0));
        a.aim(1.0, 260.0, 0.68);
        b.aim(1.0, 260.0, 0.68);
        a.step(5.0);
        b.step(0.1);
        assert_eq!(a, b);
    }
}
