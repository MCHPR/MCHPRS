use super::data::Tps;
use log::warn;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Default)]
struct AtomicTps {
    tps: AtomicU32,
    unlimited: AtomicBool,
}

impl AtomicTps {
    fn from_tps(tps: Tps) -> Self {
        match tps {
            Tps::Limited(tps) => AtomicTps {
                tps: AtomicU32::new(tps),
                unlimited: AtomicBool::new(false),
            },
            Tps::Unlimited => AtomicTps {
                tps: AtomicU32::new(0),
                unlimited: AtomicBool::new(true),
            },
        }
    }

    fn update(&self, tps: Tps) {
        match tps {
            Tps::Limited(tps) => {
                self.tps.store(tps, Ordering::SeqCst);
                self.unlimited.store(false, Ordering::SeqCst);
            }
            Tps::Unlimited => self.unlimited.store(true, Ordering::SeqCst),
        }
    }
}

struct MonitorData {
    tps: AtomicTps,
    ticks_passed: Arc<AtomicU32>,
    too_slow: AtomicBool,
    ticking: AtomicBool,
    running: AtomicBool,
    timings_record: Mutex<Vec<u32>>,
}

#[derive(Debug)]
pub struct TimingsReport {
    pub ten_s: f32,
    pub one_m: f32,
    pub five_m: f32,
    pub fifteen_m: f32,
}

pub struct TimingsMonitor {
    data: Arc<MonitorData>,
    monitor_thread: Option<JoinHandle<()>>,
}

impl TimingsMonitor {
    pub fn new(tps: Tps) -> TimingsMonitor {
        let data = Arc::new(MonitorData {
            ticks_passed: Default::default(),
            running: AtomicBool::new(true),
            too_slow: Default::default(),
            ticking: Default::default(),
            timings_record: Default::default(),
            tps: AtomicTps::from_tps(tps),
        });
        let monitor_thread = Some(Self::run_thread(data.clone()));
        TimingsMonitor {
            data,
            monitor_thread,
        }
    }

    pub fn stop(&mut self) {
        self.data.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.monitor_thread.take() {
            if handle.join().is_err() {
                warn!("Failed to join monitor thread handle");
            }
        }
    }

    pub fn generate_report(&self) -> Option<TimingsReport> {
        let records = self.data.timings_record.lock().unwrap();
        if records.is_empty() {
            return None;
        }

        let mut ticks_10s = 0;
        let mut ticks_1m = 0;
        let mut ticks_5m = 0;
        let mut ticks_15m = 0;
        for (i, ticks) in records.iter().enumerate() {
            if i < 20 {
                ticks_10s += *ticks;
            }
            if i < 120 {
                ticks_1m += *ticks;
            }
            if i < 600 {
                ticks_5m += *ticks;
            }
            ticks_15m += *ticks;
        }

        Some(TimingsReport {
            ten_s: ticks_10s as f32 / records.len().min(20) as f32 * 2.0,
            one_m: ticks_1m as f32 / records.len().min(120) as f32 * 2.0,
            five_m: ticks_5m as f32 / records.len().min(600) as f32 * 2.0,
            fifteen_m: ticks_15m as f32 / records.len() as f32 * 2.0,
        })
    }

    pub fn set_tps(&self, new_tps: Tps) {
        self.data.tps.update(new_tps);
        self.data.too_slow.store(false, Ordering::SeqCst);
    }

    pub fn tick(&self) {
        self.data.ticks_passed.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_running_behind(&self) -> bool {
        self.data.too_slow.load(Ordering::SeqCst)
    }

    pub fn set_ticking(&self, ticking: bool) {
        self.data.ticking.store(ticking, Ordering::Relaxed);
    }

    fn run_thread(data: Arc<MonitorData>) -> JoinHandle<()> {
        thread::spawn(move || {
            let mut last_tps = data.tps.tps.load(Ordering::SeqCst);
            let mut last_ticks_count = data.ticks_passed.load(Ordering::SeqCst);
            let mut was_ticking_before = data.ticking.load(Ordering::SeqCst);
            loop {
                thread::sleep(Duration::from_millis(500));
                if !data.running.load(Ordering::SeqCst) {
                    return;
                }

                let tps = data.tps.tps.load(Ordering::SeqCst);
                let ticking = data.ticking.load(Ordering::SeqCst);
                if !(ticking && was_ticking_before) || tps != last_tps {
                    was_ticking_before = ticking;
                    last_tps = tps;
                    continue;
                }

                let ticks_count = data.ticks_passed.load(Ordering::SeqCst);
                if ticks_count == 0 {
                    continue;
                }

                let ticks_passed = ticks_count - last_ticks_count;

                // 5% threshold
                if data.tps.unlimited.load(Ordering::SeqCst) || ticks_passed < (tps / 2) * 95 / 100
                {
                    data.too_slow.store(true, Ordering::SeqCst);
                    // warn!(
                    //     "running behind by {} ticks",
                    //     ((tps / 2) * 95 / 100) - ticks_passed
                    // );
                } else {
                    data.too_slow.store(false, Ordering::SeqCst);
                }

                // The timings record will only go back 15 minutes.
                // This means that, with the 500ms interval, the timings record will
                // have a max size of 1800 entries.
                let mut timings_record = data.timings_record.lock().unwrap();
                if timings_record.len() == 1800 {
                    timings_record.pop();
                }
                timings_record.insert(0, ticks_passed);

                last_ticks_count = ticks_count;
            }
        })
    }
}

impl Drop for TimingsMonitor {
    fn drop(&mut self) {
        // Joining the thread in drop is a bad idea so we just let it detach
        self.data.running.store(false, Ordering::SeqCst);
    }
}
