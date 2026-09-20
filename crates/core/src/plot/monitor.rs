use mchprs_save_data::plot_data::Tps;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(500);
const HISTORY_DURATION: Duration = Duration::from_secs(15 * 60);
const MIN_OBSERVATION_DURATION: Duration = Duration::from_millis(1500);
const MIN_EXPECTED_TICKS: f64 = 3.0;
const OBSERVATION_EXPECTED_TICKS: f64 = 20.0;

struct TimingSample {
    start: Instant,
    end: Instant,
    ticks: u64,
}

#[derive(Debug)]
pub struct TimingsReport {
    pub ten_s: f32,
    pub one_m: f32,
    pub five_m: f32,
    pub fifteen_m: f32,
}

pub struct TimingsMonitor {
    samples: VecDeque<TimingSample>,
    sample_start: Instant,
    sample_ticks: u64,
    observation_start: Instant,
    observation_ticks: u64,
    running_behind: bool,
}

impl TimingsMonitor {
    pub fn new(now: Instant) -> Self {
        Self {
            samples: VecDeque::new(),
            sample_start: now,
            sample_ticks: 0,
            observation_start: now,
            observation_ticks: 0,
            running_behind: false,
        }
    }

    pub fn reset(&mut self, now: Instant) {
        self.sample_start = now;
        self.sample_ticks = 0;
        self.observation_start = now;
        self.observation_ticks = 0;
        self.running_behind = false;
        self.prune(now);
    }

    pub fn record(&mut self, now: Instant, ticks: u64, rate: Tps) {
        self.sample_ticks += ticks;
        self.observation_ticks += ticks;
        if now.duration_since(self.sample_start) < SAMPLE_INTERVAL {
            return;
        }
        self.samples.push_back(TimingSample {
            start: self.sample_start,
            end: now,
            ticks: self.sample_ticks,
        });
        self.sample_start = now;
        self.sample_ticks = 0;
        self.prune(now);

        let elapsed = now.duration_since(self.observation_start).as_secs_f64();
        if let Tps::Limited(rate) = rate
            && rate > 0.0
        {
            let expected_ticks = f64::from(rate) * elapsed;
            if elapsed < MIN_OBSERVATION_DURATION.as_secs_f64()
                || expected_ticks < MIN_EXPECTED_TICKS
            {
                return;
            }
            self.running_behind = (self.observation_ticks as f64) < (expected_ticks * 0.95).floor();
            if expected_ticks < OBSERVATION_EXPECTED_TICKS {
                return;
            }
        } else {
            self.running_behind = false;
        }
        self.observation_start = now;
        self.observation_ticks = 0;
    }

    fn prune(&mut self, now: Instant) {
        while self
            .samples
            .front()
            .is_some_and(|sample| now.duration_since(sample.end) >= HISTORY_DURATION)
        {
            self.samples.pop_front();
        }
    }

    fn rate(&self, now: Instant, duration: Duration) -> f32 {
        let mut ticks = 0.0;
        let mut seconds = 0.0;
        for sample in self.samples.iter().rev() {
            let age = now.duration_since(sample.end);
            if age >= duration {
                break;
            }
            let elapsed = sample.end.duration_since(sample.start);
            let overlap = elapsed.min(duration - age).as_secs_f64();
            ticks += sample.ticks as f64 * overlap / elapsed.as_secs_f64();
            seconds += overlap;
        }
        if seconds == 0.0 {
            0.0
        } else {
            (ticks / seconds) as f32
        }
    }

    pub fn generate_report(&self, now: Instant) -> Option<TimingsReport> {
        self.samples
            .back()
            .filter(|sample| now.duration_since(sample.end) < HISTORY_DURATION)?;
        Some(TimingsReport {
            ten_s: self.rate(now, Duration::from_secs(10)),
            one_m: self.rate(now, Duration::from_secs(60)),
            five_m: self.rate(now, Duration::from_secs(5 * 60)),
            fifteen_m: self.rate(now, HISTORY_DURATION),
        })
    }

    pub fn is_running_behind(&self) -> bool {
        self.running_behind
    }
}
