pub mod analysis;
pub mod spec_code;
pub mod stagnation;
pub mod state;

use anyhow::Result;
use serde::Serialize;

use crate::fs::FsOps;
use crate::git::GitOps;

use analysis::{AnalysisContext, DiffAnalysis};
use stagnation::classify_pattern;
use state::{
    append_output_entry, read_state, state_dir, validate_loop_name, write_state, OutputEntry,
};

// Exit codes with business semantics
pub const EXIT_NO_CHANGES: i32 = 0;
pub const EXIT_SPEC_CHANGED: i32 = 1;
pub const EXIT_CODE_CHANGED: i32 = 2;
pub const EXIT_FIRST_RUN: i32 = 3;
pub const EXIT_STAGNATION: i32 = 4;

#[derive(Serialize)]
struct DiffResult {
    status: String,
    changed_files: Vec<String>,
    #[serde(flatten)]
    extra: serde_json::Value,
}

impl DiffResult {
    fn empty(status: &str) -> Self {
        Self {
            status: status.to_string(),
            changed_files: vec![],
            extra: serde_json::json!({"spec_files": [], "code_files": []}),
        }
    }
}

/// Service that orchestrates change detection and analysis.
///
/// Dependencies are injected via constructor. Analysis strategies
/// are pluggable via the `DiffAnalysis` trait (OCP).
pub struct CheckService {
    git: Box<dyn GitOps>,
    fs: Box<dyn FsOps>,
    analyzers: Vec<Box<dyn DiffAnalysis>>,
}

impl CheckService {
    pub fn new(
        git: Box<dyn GitOps>,
        fs: Box<dyn FsOps>,
        analyzers: Vec<Box<dyn DiffAnalysis>>,
    ) -> Self {
        Self { git, fs, analyzers }
    }

    /// Check what changed since last analysis.
    ///
    /// Exit codes: 0=no_changes, 1=spec_changed, 2=code_changed, 3=first_run, 4=stagnation
    pub fn diff(&self, loop_name: &str, spec_paths: &[String]) -> Result<i32> {
        validate_loop_name(loop_name)?;
        let state_file = state_dir(self.git.as_ref())?.join(format!("{loop_name}.state"));

        // Try reading state file; missing file means first run
        let state = match read_state(self.fs.as_ref(), &state_file) {
            Ok(s) => s,
            Err(_) => return print_and_exit(&DiffResult::empty("first_run"), EXIT_FIRST_RUN),
        };

        if !self.git.commit_exists(&state.hash)? {
            return print_and_exit(&DiffResult::empty("first_run"), EXIT_FIRST_RUN);
        }

        let current = self.git.rev_parse_head()?;

        if state.hash == current {
            return print_and_exit(&DiffResult::empty("no_changes"), EXIT_NO_CHANGES);
        }

        let changed = self.git.diff_name_only(&state.hash, &current)?;

        if changed.is_empty() {
            return print_and_exit(&DiffResult::empty("no_changes"), EXIT_NO_CHANGES);
        }

        // Classify files into spec vs code
        let mut spec_files = Vec::new();
        let mut code_files = Vec::new();
        for file in &changed {
            let is_spec = spec_paths
                .iter()
                .any(|prefix| file.starts_with(prefix.trim_end_matches('/')));
            if is_spec {
                spec_files.push(file.clone());
            } else {
                code_files.push(file.clone());
            }
        }

        // Default exit code from file classification
        let (status, mut exit) = if !spec_files.is_empty() {
            ("spec_changed", EXIT_SPEC_CHANGED)
        } else {
            ("code_changed", EXIT_CODE_CHANGED)
        };

        // Run analysis pipeline
        let ctx = AnalysisContext {
            loop_name,
            changed_files: &changed,
            spec_files: &spec_files,
            code_files: &code_files,
            state: &state,
        };

        let mut merged_extra = serde_json::json!({
            "spec_files": spec_files,
            "code_files": code_files,
        });

        for analyzer in &self.analyzers {
            let outcome = analyzer.analyze(&ctx)?;
            if let Some(override_exit) = outcome.exit_override {
                exit = override_exit;
            }
            if let serde_json::Value::Object(map) = outcome.extra_fields {
                for (k, v) in map {
                    merged_extra[k] = v;
                }
            }
        }

        let result = DiffResult {
            status: if exit == EXIT_STAGNATION {
                "stagnation".to_string()
            } else {
                status.to_string()
            },
            changed_files: changed,
            extra: merged_extra,
        };
        print_and_exit(&result, exit)
    }

    /// Record current HEAD as the last analyzed commit.
    pub fn mark(
        &self,
        loop_name: &str,
        output_hash: Option<&str>,
        status: Option<&str>,
    ) -> Result<i32> {
        validate_loop_name(loop_name)?;
        let state_file = state_dir(self.git.as_ref())?.join(format!("{loop_name}.state"));
        let hash = self.git.rev_parse_head()?;
        let ts = state::utc_timestamp();

        // Read existing state or create new one
        let mut loop_state = read_state(self.fs.as_ref(), &state_file).unwrap_or_default();

        loop_state.hash = hash.clone();
        loop_state.timestamp = ts.clone();

        // Update idle count based on status
        match status {
            Some("idle") => loop_state.idle_count += 1,
            Some("active") => loop_state.idle_count = 0,
            _ => {}
        }

        // Append output hash if provided
        if let Some(simhash) = output_hash {
            append_output_entry(
                &mut loop_state,
                OutputEntry {
                    simhash: simhash.to_string(),
                    category: state::OutputCategory::GapAnalysis,
                    timestamp: ts.clone(),
                },
            );
        }

        write_state(self.fs.as_ref(), &state_file, &loop_state)?;
        println!(
            "marked {loop_name}: {hash} at {ts} (idle_count: {})",
            loop_state.idle_count
        );
        Ok(0)
    }

    /// Show state of all loops.
    pub fn status(&self) -> Result<i32> {
        let dir = state_dir(self.git.as_ref())?;

        let files = match self.fs.list_files(&dir, "state") {
            Ok(f) => f,
            Err(_) => {
                println!("(no loop states found)");
                return Ok(0);
            }
        };

        if files.is_empty() {
            println!("(no loop states found)");
            return Ok(0);
        }

        println!("{:<20}  {:<9}  TIMESTAMP", "LOOP", "HASH");
        println!("{}", "-".repeat(55));

        for file in &files {
            let name = file
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            if let Ok(state) = read_state(self.fs.as_ref(), file) {
                let short = &state.hash[..7.min(state.hash.len())];
                println!("{:<20}  {:<9}  {}", name, short, state.timestamp);
            }
        }

        Ok(0)
    }

    /// Pipeline health report across all loops.
    pub fn health(&self) -> Result<i32> {
        use crate::cmd::simhash;

        let dir = state_dir(self.git.as_ref())?;
        let files = match self.fs.list_files(&dir, "state") {
            Ok(f) => f,
            Err(_) => {
                println!(r#"{{"loops":[],"overall":"healthy"}}"#);
                return Ok(0);
            }
        };

        if files.is_empty() {
            println!(r#"{{"loops":[],"overall":"healthy"}}"#);
            return Ok(0);
        }

        let mut loops = Vec::new();
        let mut any_stagnation = false;

        for file in &files {
            let name = file
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            if let Ok(loop_state) = read_state(self.fs.as_ref(), file) {
                let history = &loop_state.output_history;
                let pattern = if history.len() >= 2 {
                    let latest = &history[history.len() - 1];
                    simhash::parse_simhash(&latest.simhash).and_then(|latest_hash| {
                        classify_pattern(
                            history,
                            latest_hash,
                            stagnation::DEFAULT_SIMILARITY_THRESHOLD,
                        )
                    })
                } else {
                    None
                };

                if pattern.is_some() {
                    any_stagnation = true;
                }

                loops.push(serde_json::json!({
                    "name": name,
                    "hash": &loop_state.hash[..7.min(loop_state.hash.len())],
                    "timestamp": loop_state.timestamp,
                    "history_len": history.len(),
                    "pattern_type": pattern,
                    "recommended_persona": pattern.map(|p| p.recommended_persona()),
                }));
            }
        }

        let overall = if any_stagnation {
            "degraded"
        } else {
            "healthy"
        };
        let out = serde_json::json!({ "loops": loops, "overall": overall });
        println!("{out}");
        Ok(0)
    }
}

fn print_and_exit(result: &DiffResult, exit: i32) -> Result<i32> {
    println!("{}", serde_json::to_string(result)?);
    Ok(exit)
}
