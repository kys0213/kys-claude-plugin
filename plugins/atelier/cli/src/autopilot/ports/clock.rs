use std::sync::Mutex;

use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct StdClock;

impl Clock for StdClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub struct FixedClock {
    state: Mutex<DateTime<Utc>>,
}

impl FixedClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            state: Mutex::new(now),
        }
    }

    pub fn set(&self, now: DateTime<Utc>) {
        *self.state.lock().expect("clock poisoned") = now;
    }

    pub fn advance(&self, by: chrono::Duration) {
        let mut g = self.state.lock().expect("clock poisoned");
        *g += by;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.state.lock().expect("clock poisoned")
    }
}
