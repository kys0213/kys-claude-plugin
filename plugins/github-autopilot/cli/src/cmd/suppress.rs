//! Fingerprint suppression CLI (spec §6.3).
//!
//! Thin adapter over [`SuppressionRepo`]. The CLI never decides what
//! `reason` strings mean — agents own that vocabulary (`unmatched_watch`,
//! `rejected_by_human`, ...). This layer only persists `(fingerprint,
//! reason) -> until` and reports whether `now` is still inside the window.

use std::io::Write;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};

use crate::ports::clock::Clock;
use crate::ports::task_store::SuppressionRepo;

#[derive(Subcommand)]
pub enum SuppressCommands {
    /// Suppress alerts for `(fingerprint, reason)` until the given timestamp
    Add(AddArgs),
    /// Exit 0 if currently suppressed, exit 1 otherwise (script-friendly)
    Check(CheckArgs),
    /// Remove a suppression entry
    Clear(ClearArgs),
}

#[derive(Args)]
pub struct AddArgs {
    #[arg(long)]
    pub fingerprint: String,
    #[arg(long)]
    pub reason: String,
    /// RFC3339 / ISO8601 timestamp (e.g. `2026-05-04T12:00:00Z`)
    #[arg(long)]
    pub until: String,
}

#[derive(Args)]
pub struct CheckArgs {
    #[arg(long)]
    pub fingerprint: String,
    #[arg(long)]
    pub reason: String,
}

#[derive(Args)]
pub struct ClearArgs {
    #[arg(long)]
    pub fingerprint: String,
    #[arg(long)]
    pub reason: String,
}

pub struct SuppressService<'a> {
    store: &'a dyn SuppressionRepo,
    clock: &'a dyn Clock,
}

impl<'a> SuppressService<'a> {
    pub fn new(store: &'a dyn SuppressionRepo, clock: &'a dyn Clock) -> Self {
        Self { store, clock }
    }

    pub fn add(
        &self,
        fingerprint: &str,
        reason: &str,
        until: &str,
        out: &mut dyn Write,
    ) -> Result<i32> {
        let parsed: DateTime<Utc> = parse_until(until)?;
        self.store
            .suppress(fingerprint, reason, parsed)
            .with_context(|| format!("suppressing fingerprint '{fingerprint}'"))?;
        writeln!(
            out,
            "suppressed fingerprint='{fingerprint}' reason='{reason}' until={}",
            parsed.to_rfc3339()
        )?;
        Ok(0)
    }

    pub fn check(&self, fingerprint: &str, reason: &str, out: &mut dyn Write) -> Result<i32> {
        let now = self.clock.now();
        let active = self
            .store
            .is_suppressed(fingerprint, reason, now)
            .with_context(|| format!("checking suppression for '{fingerprint}'"))?;
        if active {
            writeln!(out, "suppressed")?;
            Ok(0)
        } else {
            writeln!(out, "not suppressed")?;
            Ok(1)
        }
    }

    pub fn clear(&self, fingerprint: &str, reason: &str, out: &mut dyn Write) -> Result<i32> {
        self.store
            .clear(fingerprint, reason)
            .with_context(|| format!("clearing suppression for '{fingerprint}'"))?;
        writeln!(out, "cleared fingerprint='{fingerprint}' reason='{reason}'")?;
        Ok(0)
    }
}

fn parse_until(raw: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .with_context(|| format!("parsing --until '{raw}' as RFC3339"))
}

pub fn suppress_service<'a>(
    store: &'a dyn SuppressionRepo,
    clock: &'a dyn Clock,
) -> SuppressService<'a> {
    SuppressService::new(store, clock)
}
