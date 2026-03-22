pub mod repository;
pub mod schema;

use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
        )?;
        Ok(Self { conn })
    }

    pub fn initialize(&self) -> Result<()> {
        schema::create_tables(&self.conn)?;
        schema::migrate_v2(&self.conn)?;
        schema::migrate_v3(&self.conn)?;
        schema::migrate_v4(&self.conn)?;
        schema::migrate_v5(&self.conn)?;
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::*;
    use crate::core::repository::*;
    use std::sync::{Arc, Barrier};

    fn setup_db() -> Database {
        let db = Database::open(Path::new(":memory:")).unwrap();
        db.initialize().unwrap();
        db
    }

    fn add_repo(db: &Database) -> String {
        db.repo_add("https://github.com/test/repo", "test-repo")
            .unwrap()
    }

    fn make_queue_item(repo_id: &str, work_id: &str) -> QueueItemRow {
        let now = chrono::Utc::now().to_rfc3339();
        QueueItemRow {
            work_id: work_id.to_string(),
            repo_id: repo_id.to_string(),
            queue_type: QueueType::Issue,
            phase: QueuePhase::Pending,
            title: Some("test".to_string()),
            skip_reason: None,
            created_at: now.clone(),
            updated_at: now,
            task_kind: crate::core::phase::TaskKind::Analyze,
            github_number: 1,
            metadata_json: None,
            failure_count: 0,
            escalation_level: 0,
        }
    }

    #[test]
    fn busy_timeout_is_set() {
        let db = setup_db();
        let timeout: i64 = db
            .conn()
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(timeout, 5000);
    }

    #[test]
    fn queue_increment_failure_returns_correct_count() {
        let db = setup_db();
        let repo_id = add_repo(&db);
        let item = make_queue_item(&repo_id, "work-1");
        db.queue_upsert(&item).unwrap();

        let count1 = db.queue_increment_failure("work-1").unwrap();
        assert_eq!(count1, 1);

        let count2 = db.queue_increment_failure("work-1").unwrap();
        assert_eq!(count2, 2);

        let count3 = db.queue_increment_failure("work-1").unwrap();
        assert_eq!(count3, 3);
    }

    #[test]
    fn feedback_upsert_insert_then_update_atomically() {
        let db = setup_db();
        let repo_id = add_repo(&db);
        let pattern = NewFeedbackPattern {
            repo_id: repo_id.clone(),
            pattern_type: "style".to_string(),
            suggestion: "use snake_case".to_string(),
            source: "review".to_string(),
        };

        // First insert
        let id1 = db.feedback_upsert(&pattern).unwrap();
        assert!(!id1.is_empty());

        // Second upsert returns existing id
        let id2 = db.feedback_upsert(&pattern).unwrap();
        assert_eq!(id1, id2);

        // Verify occurrence_count incremented
        let patterns = db.feedback_list(&repo_id).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].occurrence_count, 2);
    }

    #[test]
    fn concurrent_increment_failure_on_file_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Set up initial data
        {
            let db = Database::open(&db_path).unwrap();
            db.initialize().unwrap();
            let repo_id = add_repo(&db);
            let item = make_queue_item(&repo_id, "work-concurrent");
            db.queue_upsert(&item).unwrap();
        }

        let iterations = 10;
        let threads = 4;
        let barrier = Arc::new(Barrier::new(threads));

        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let path = db_path.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let db = Database::open(&path).unwrap();
                    barrier.wait();
                    for _ in 0..iterations {
                        db.queue_increment_failure("work-concurrent").unwrap();
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Verify total increments
        let db = Database::open(&db_path).unwrap();
        let count = db.queue_get_failure_count("work-concurrent").unwrap();
        assert_eq!(count, (threads * iterations) as i32);
    }

    #[test]
    fn concurrent_feedback_upsert_on_file_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Set up initial data
        let repo_id;
        {
            let db = Database::open(&db_path).unwrap();
            db.initialize().unwrap();
            repo_id = add_repo(&db);
        }

        let iterations = 10;
        let threads = 4;
        let barrier = Arc::new(Barrier::new(threads));

        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let path = db_path.clone();
                let barrier = Arc::clone(&barrier);
                let rid = repo_id.clone();
                std::thread::spawn(move || {
                    let db = Database::open(&path).unwrap();
                    barrier.wait();
                    for _ in 0..iterations {
                        let pattern = NewFeedbackPattern {
                            repo_id: rid.clone(),
                            pattern_type: "style".to_string(),
                            suggestion: "use snake_case".to_string(),
                            source: "review".to_string(),
                        };
                        db.feedback_upsert(&pattern).unwrap();
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Verify single row with correct occurrence_count
        let db = Database::open(&db_path).unwrap();
        let patterns = db.feedback_list(&repo_id).unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].occurrence_count, (threads * iterations) as i32);
    }
}
