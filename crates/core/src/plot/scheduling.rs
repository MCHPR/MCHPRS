use mchprs_save_data::plot_data::{Tps, WorldSendRate};
use std::time::{Duration, Instant};

pub const MAX_BATCH_DURATION: Duration = Duration::from_millis(10);
const MAX_TICK_BACKLOG: Duration = Duration::from_secs(2);
pub const PLAYER_UPDATE_INTERVAL: Duration = Duration::from_millis(50);

pub enum RateSchedule {
    Unlimited,
    Limited {
        rate: f64,
        max_backlog: f64,
        last_update: Instant,
        backlog: f64,
    },
}

impl RateSchedule {
    pub fn for_ticks(tps: Tps, now: Instant) -> Self {
        match tps {
            Tps::Limited(rate) => {
                let max_backlog = (f64::from(rate) * MAX_TICK_BACKLOG.as_secs_f64()).max(1.0);
                Self::limited(rate, max_backlog, now)
            }
            Tps::Unlimited => Self::Unlimited,
        }
    }

    pub fn for_sends(rate: WorldSendRate, now: Instant) -> Self {
        Self::limited(rate.0, 1.0, now)
    }

    fn limited(rate: f32, max_backlog: f64, now: Instant) -> Self {
        Self::Limited {
            rate: f64::from(rate),
            max_backlog,
            last_update: now,
            backlog: 0.0,
        }
    }

    pub fn due(&mut self, now: Instant) -> u64 {
        let Self::Limited {
            rate,
            max_backlog,
            last_update,
            backlog,
        } = self
        else {
            return u64::MAX;
        };
        let elapsed = now.duration_since(*last_update);
        *last_update = now;
        *backlog = (*backlog + elapsed.as_secs_f64() * *rate).min(*max_backlog);
        *backlog as u64
    }

    pub fn complete(&mut self, count: u64) {
        if let Self::Limited { backlog, .. } = self {
            *backlog = (*backlog - count as f64).max(0.0);
        }
    }

    pub fn wait(&self, now: Instant) -> Duration {
        let Self::Limited {
            rate,
            last_update,
            backlog,
            ..
        } = self
        else {
            return Duration::ZERO;
        };
        if *backlog >= 1.0 {
            return Duration::ZERO;
        }
        Duration::try_from_secs_f64((1.0 - backlog) / rate)
            .unwrap_or(Duration::MAX)
            .saturating_sub(now.duration_since(*last_update))
    }
}
