//! Central fixed simulation clock. Frontends supply time, never tick counts.
use std::time::Duration;

pub const FIXED_HZ: u32 = 30;
/// One tick, rounded down to integer nanoseconds (error < 1 ns per tick).
pub const FIXED_DT: Duration = Duration::from_nanos(1_000_000_000 / FIXED_HZ as u64);
pub const MAX_CATCH_UP_TICKS: usize = 5;

#[derive(Default)]
pub(crate) struct FixedClock {
    remainder: Duration,
}

impl FixedClock {
    pub fn advance(&mut self, elapsed: Duration) -> usize {
        let debt = self.remainder.as_nanos() + elapsed.as_nanos();
        let period = FIXED_DT.as_nanos();
        let ticks = (debt / period).min(MAX_CATCH_UP_TICKS as u128) as usize;
        // Discard whole old ticks beyond the budget; retain only fractional debt.
        self.remainder = Duration::from_nanos((debt % period) as u64);
        ticks
    }
}
