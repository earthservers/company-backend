use bson::doc;
use futures::StreamExt;
use company_result::Result;

use crate::AuditLogEntry;
use crate::MongoDb;

use super::AbstractAuditLog;

static COL: &str = "audit_log";

#[async_trait]
impl AbstractAuditLog for MongoDb {
    async fn insert_audit_log(&self, entry: &AuditLogEntry) -> Result<()> {
        query!(self, insert_one, COL, &entry).map(|_| ())
    }

    async fn fetch_audit_log(&self, limit: i64) -> Result<Vec<AuditLogEntry>> {
        Ok(self
            .col::<AuditLogEntry>(COL)
            .find(doc! {})
            .sort(doc! { "timestamp": -1 })
            .limit(limit)
            .await
            .map_err(|_| create_database_error!("find", COL))?
            .filter_map(|s| async {
                if cfg!(debug_assertions) {
                    Some(s.unwrap())
                } else {
                    s.ok()
                }
            })
            .collect()
            .await)
    }
}
