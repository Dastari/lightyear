use crate::delta::Diffable;
use avian2d::prelude::{AngularVelocity, LinearVelocity, Position, Rotation};

impl Diffable<Position> for Position {
    fn base_value() -> Self {
        Position::default()
    }

    fn diff(&self, new: &Self) -> Position {
        Position(new.0 - self.0)
    }

    fn apply_diff(&mut self, delta: &Position) {
        self.0 += **delta;
    }
}

impl Diffable<Rotation> for Rotation {
    fn base_value() -> Self {
        Rotation::default()
    }

    fn diff(&self, new: &Self) -> Rotation {
        Rotation::radians(self.angle_between(*new))
    }

    fn apply_diff(&mut self, delta: &Rotation) {
        *self = self.add_angle_fast(delta.as_radians());
    }
}

impl Diffable<LinearVelocity> for LinearVelocity {
    fn base_value() -> Self {
        LinearVelocity::default()
    }

    fn diff(&self, new: &Self) -> LinearVelocity {
        LinearVelocity(new.0 - self.0)
    }

    fn apply_diff(&mut self, delta: &LinearVelocity) {
        self.0 += **delta;
    }
}

impl Diffable<AngularVelocity> for AngularVelocity {
    fn base_value() -> Self {
        AngularVelocity::default()
    }

    fn diff(&self, new: &Self) -> AngularVelocity {
        AngularVelocity(new.0 - self.0)
    }

    fn apply_diff(&mut self, delta: &AngularVelocity) {
        self.0 += **delta;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use avian2d::math::{Scalar, Vector};

    #[test]
    fn linear_velocity_diff_round_trip_preserves_values() {
        let start = LinearVelocity(Vector::new(-12.5 as Scalar, 3.25 as Scalar));
        let end = LinearVelocity(Vector::new(0.125 as Scalar, -9.75 as Scalar));

        let delta = start.diff(&end);
        let mut round_trip = start;
        round_trip.apply_diff(&delta);

        assert_eq!(round_trip, end);
    }

    #[test]
    fn angular_velocity_diff_round_trip_preserves_values() {
        let start = AngularVelocity(-12.5 as Scalar);
        let end = AngularVelocity(3.125 as Scalar);

        let delta = start.diff(&end);
        let mut round_trip = start;
        round_trip.apply_diff(&delta);

        assert_eq!(round_trip, end);
    }
}
