use company_result::Result;

use crate::AuditLogEntry;
use crate::ReferenceDb;

use super::AbstractAuditLog;

#[async_trait]
impl AbstractAuditLog for ReferenceDb {
    async fn insert_audit_log(&self, entry: &AuditLogEntry) -> Result<()> {
        let mut log = self.audit_log.lock().await;
        log.insert(entry.id.to_string(), entry.clone());
        Ok(())
    }

    async fn fetch_audit_log(&self, limit: i64) -> Result<Vec<AuditLogEntry>> {
        let log = self.audit_log.lock().await;
        let mut entries: Vec<AuditLogEntry> = log.values().cloned().collect();
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        entries.truncate(limit as usize);
        Ok(entries)
    }
}
