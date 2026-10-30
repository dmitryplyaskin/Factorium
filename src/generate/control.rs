//! Cooperative cancellation and observable progress for CPU-bound generation.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Result, ensure};
use serde::Serialize;

#[derive(Clone, Default, Serialize)]
pub struct GenerationProgress {
    pub machines: usize,
    pub candidate: usize,
    pub candidates: usize,
    pub stage: &'static str,
}

#[derive(Default)]
pub struct GenerationControl {
    cancelled: AtomicBool,
    progress: Mutex<GenerationProgress>,
}

impl GenerationControl {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn progress(&self) -> GenerationProgress {
        self.progress.lock().unwrap().clone()
    }

    pub(super) fn check(&self) -> Result<()> {
        ensure!(!self.is_cancelled(), "Generation cancelled");
        Ok(())
    }

    pub(super) fn update(&self, update: impl FnOnce(&mut GenerationProgress)) -> Result<()> {
        self.check()?;
        update(&mut self.progress.lock().unwrap());
        Ok(())
    }
}
