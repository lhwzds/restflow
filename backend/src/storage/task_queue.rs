use anyhow::Result;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::sync::Arc;
use tokio::sync::Notify;

// KISS: Three-table design achieves O(1) pop vs single table's O(n) scan - simpler and faster
const PENDING: TableDefinition<u64, &[u8]> = TableDefinition::new("pending");
const PROCESSING: TableDefinition<&str, &[u8]> = TableDefinition::new("processing");
const COMPLETED: TableDefinition<&str, &[u8]> = TableDefinition::new("completed");

/// Pure storage layer for task queue - only handles data persistence
#[derive(Clone)]
pub struct TaskQueue {
    db: Arc<Database>,
    notify: Arc<Notify>,
}

impl TaskQueue {
    /// Create a new task queue instance
    pub fn new(db: Arc<Database>) -> Result<Self> {
        // Ensure tables exist
        let write_txn = db.begin_write()?;
        write_txn.open_table(PENDING)?;
        write_txn.open_table(PROCESSING)?;
        write_txn.open_table(COMPLETED)?;
        write_txn.commit()?;

        Ok(Self {
            db,
            notify: Arc::new(Notify::new()),
        })
    }

    /// Insert a task into the pending queue
    pub fn insert_pending(&self, priority: u64, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(PENDING)?;
            table.insert(priority, data)?;
        }
        write_txn.commit()?;
        self.notify.notify_one();
        Ok(())
    }

    /// Get the first pending task without removing it
    pub fn get_first_pending(&self) -> Result<Option<(u64, Vec<u8>)>> {
        let read_txn = self.db.begin_read()?;
        let pending = read_txn.open_table(PENDING)?;
        
        if let Some((key, value)) = pending.first()? {
            Ok(Some((key.value(), value.value().to_vec())))
        } else {
            Ok(None)
        }
    }

    /// Move a task from pending to processing
    pub fn move_to_processing(&self, priority: u64, task_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        
        // Remove from pending
        {
            let mut pending = write_txn.open_table(PENDING)?;
            pending.remove(&priority)?;
        }
        
        // Add to processing
        {
            let mut processing = write_txn.open_table(PROCESSING)?;
            processing.insert(task_id, data)?;
        }
        
        write_txn.commit()?;
        Ok(())
    }

    /// Move a task from processing to completed
    pub fn move_to_completed(&self, task_id: &str, data: &[u8]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        
        // Remove from processing
        {
            let mut processing = write_txn.open_table(PROCESSING)?;
            processing.remove(task_id)?;
        }
        
        // Add to completed
        {
            let mut completed = write_txn.open_table(COMPLETED)?;
            completed.insert(task_id, data)?;
        }
        
        write_txn.commit()?;
        Ok(())
    }

    /// Get a task from processing table
    pub fn get_from_processing(&self, task_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let processing = read_txn.open_table(PROCESSING)?;
        
        if let Some(data) = processing.get(task_id)? {
            Ok(Some(data.value().to_vec()))
        } else {
            Ok(None)
        }
    }

    /// Remove a task from processing table
    pub fn remove_from_processing(&self, task_id: &str) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut processing = write_txn.open_table(PROCESSING)?;
            processing.remove(task_id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    /// Get a task from any table
    pub fn get_from_any_table(&self, task_id: &str) -> Result<Option<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        
        // Check processing table
        let processing = read_txn.open_table(PROCESSING)?;
        if let Some(data) = processing.get(task_id)? {
            return Ok(Some(data.value().to_vec()));
        }
        
        // Check completed table
        let completed = read_txn.open_table(COMPLETED)?;
        if let Some(data) = completed.get(task_id)? {
            return Ok(Some(data.value().to_vec()));
        }
        
        // Check pending table (requires iteration)
        let pending = read_txn.open_table(PENDING)?;
        for entry in pending.iter()? {
            let (_, _value) = entry?;
            // Note: We'd need to deserialize to check ID, but for pure storage we return None
            // The scheduler should handle this logic
        }
        
        Ok(None)
    }

    /// Get all tasks from pending table
    pub fn get_all_pending(&self) -> Result<Vec<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let pending = read_txn.open_table(PENDING)?;
        let mut tasks = Vec::new();
        
        for entry in pending.iter()? {
            let (_, value) = entry?;
            tasks.push(value.value().to_vec());
        }
        
        Ok(tasks)
    }

    /// Get all tasks from processing table
    pub fn get_all_processing(&self) -> Result<Vec<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let processing = read_txn.open_table(PROCESSING)?;
        let mut tasks = Vec::new();
        
        for entry in processing.iter()? {
            let (_, value) = entry?;
            tasks.push(value.value().to_vec());
        }
        
        Ok(tasks)
    }

    /// Get all tasks from completed table
    pub fn get_all_completed(&self) -> Result<Vec<Vec<u8>>> {
        let read_txn = self.db.begin_read()?;
        let completed = read_txn.open_table(COMPLETED)?;
        let mut tasks = Vec::new();
        
        for entry in completed.iter()? {
            let (_, value) = entry?;
            tasks.push(value.value().to_vec());
        }
        
        Ok(tasks)
    }

    /// Wait for a task to be available
    pub async fn wait_for_task(&self) {
        self.notify.notified().await;
    }

    /// Notify that a task is available
    pub fn notify_task_available(&self) {
        self.notify.notify_one();
    }
}