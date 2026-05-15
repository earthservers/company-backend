use bson::doc;
use futures::StreamExt;
use company_result::Result;

use company_models::v0::ReportStatus;

use crate::MongoDb;
use crate::Report;

use super::AbstractReport;

static COL: &str = "safety_reports";

#[async_trait]
impl AbstractReport for MongoDb {
    /// Insert a new report into the database
    async fn insert_report(&self, report: &Report) -> Result<()> {
        query!(self, insert_one, COL, &report).map(|_| ())
    }

    /// Fetch all reports, optionally filtered by status
    async fn fetch_reports(&self, status: Option<String>) -> Result<Vec<Report>> {
        let filter = if let Some(status) = status {
            doc! { "status": status }
        } else {
            doc! {}
        };

        Ok(self
            .col::<Report>(COL)
            .find(filter)
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

    /// Fetch a single report by ID
    async fn fetch_report(&self, id: &str) -> Result<Report> {
        query!(self, find_one_by_id, COL, id)?.ok_or_else(|| create_error!(NotFound))
    }

    /// Update a report's status
    async fn update_report_status(&self, id: &str, status: ReportStatus) -> Result<()> {
        let status_doc = bson::to_document(&status)
            .map_err(|_| create_database_error!("to_document", "report_status"))?;

        self.col::<bson::Document>(COL)
            .update_one(
                doc! { "_id": id },
                doc! { "$set": status_doc },
            )
            .await
            .map(|_| ())
            .map_err(|_| create_database_error!("update_one", COL))
    }

    /// Count reports with "Created" status (pending triage)
    async fn count_pending_reports(&self) -> Result<u64> {
        self.col::<Report>(COL)
            .count_documents(doc! { "status": "Created" })
            .await
            .map_err(|_| create_database_error!("count_documents", COL))
    }
}
