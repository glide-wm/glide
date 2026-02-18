use std::time::Instant;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SpringAnimation {
    initial_value: f64,
    target_value: f64,
    initial_velocity: f64,
    start_time: Instant,
    response: f64,
    damping_fraction: f64,
    omega_n: f64,
    omega_d: f64,
    zeta: f64,
}

impl SpringAnimation {
    pub fn new(
        initial_value: f64,
        target_value: f64,
        initial_velocity: f64,
        response: f64,
        damping_fraction: f64,
        now: Instant,
    ) -> Self {
        let omega_n = 2.0 * std::f64::consts::PI / response;
        let zeta = damping_fraction;
        let omega_d = omega_n * (1.0 - zeta * zeta).max(0.0).sqrt();
        SpringAnimation {
            initial_value,
            target_value,
            initial_velocity,
            start_time: now,
            response,
            damping_fraction,
            omega_n,
            omega_d,
            zeta,
        }
    }

    pub fn with_defaults(initial_value: f64, target_value: f64, now: Instant) -> Self {
        Self::new(initial_value, target_value, 0.0, 0.5, 1.0, now)
    }

    pub fn retarget(&mut self, new_target: f64, now: Instant) {
        let current = self.value_at(now);
        let vel = self.velocity_at(now);
        self.initial_value = current;
        self.target_value = new_target;
        self.initial_velocity = vel;
        self.start_time = now;
    }

    pub fn value_at(&self, time: Instant) -> f64 {
        let t = time.duration_since(self.start_time).as_secs_f64();
        let x0 = self.initial_value - self.target_value;
        let v0 = self.initial_velocity;

        let displacement = if self.zeta >= 1.0 {
            let decay = (-self.omega_n * t).exp();
            decay * (x0 + (v0 + self.omega_n * x0) * t)
        } else {
            let decay = (-self.zeta * self.omega_n * t).exp();
            let cos_part = x0 * (self.omega_d * t).cos();
            let sin_part =
                ((v0 + self.zeta * self.omega_n * x0) / self.omega_d) * (self.omega_d * t).sin();
            decay * (cos_part + sin_part)
        };

        self.target_value + displacement
    }

    pub fn velocity_at(&self, time: Instant) -> f64 {
        let t = time.duration_since(self.start_time).as_secs_f64();
        let x0 = self.initial_value - self.target_value;
        let v0 = self.initial_velocity;

        if self.zeta >= 1.0 {
            let decay = (-self.omega_n * t).exp();
            let a = v0 + self.omega_n * x0;
            decay * (a - self.omega_n * (x0 + a * t))
        } else {
            let decay = (-self.zeta * self.omega_n * t).exp();
            let b = (v0 + self.zeta * self.omega_n * x0) / self.omega_d;
            let cos_t = (self.omega_d * t).cos();
            let sin_t = (self.omega_d * t).sin();
            decay
                * ((-self.zeta * self.omega_n) * (x0 * cos_t + b * sin_t)
                    + (-x0 * self.omega_d * sin_t + b * self.omega_d * cos_t))
        }
    }

    pub fn is_complete(&self, time: Instant) -> bool {
        let t = time.duration_since(self.start_time).as_secs_f64();
        if t < 0.01 {
            return false;
        }
        let val = self.value_at(time);
        let vel = self.velocity_at(time);
        (val - self.target_value).abs() < 0.5 && vel.abs() < 0.5
    }

    pub fn target(&self) -> f64 {
        self.target_value
    }

    pub fn current(&self, now: Instant) -> f64 {
        self.value_at(now)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn critically_damped_converges() {
        let now = Instant::now();
        let spring = SpringAnimation::new(0.0, 100.0, 0.0, 0.5, 1.0, now);
        let end = spring.start_time + Duration::from_secs(2);
        let val = spring.value_at(end);
        assert!((val - 100.0).abs() < 1.0);
        assert!(spring.is_complete(end));
    }

    #[test]
    fn underdamped_oscillates() {
        let now = Instant::now();
        let spring = SpringAnimation::new(0.0, 100.0, 0.0, 0.5, 0.5, now);
        let mid = spring.start_time + Duration::from_millis(200);
        let val = spring.value_at(mid);
        assert!(val > 50.0);
    }

    #[test]
    fn retarget_preserves_continuity() {
        let now = Instant::now();
        let mut spring = SpringAnimation::new(0.0, 100.0, 0.0, 0.5, 1.0, now);
        let mid = now;
        let val_before = spring.value_at(mid);
        spring.retarget(200.0, now);
        let val_after = spring.value_at(now);
        assert!((val_before - val_after).abs() < 5.0);
    }
}
