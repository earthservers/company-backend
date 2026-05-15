use company_result::Result;

use crate::AuditLogEntry;

#[cfg(feature = "mongodb")]
mod mongodb;
mod reference;

#[async_trait]
pub trait AbstractAuditLog: Sync + Send {
    /// Insert a new audit log entry
    async fn insert_audit_log(&self, entry: &AuditLogEntry) -> Result<()>;

    /// Fetch recent audit log entries, most recent first
    async fn fetch_audit_log(&self, limit: i64) -> Result<Vec<AuditLogEntry>>;
}
