use company_result::Result;

use company_models::v0::ReportStatus;

use crate::Report;

#[cfg(feature = "mongodb")]
mod mongodb;
mod reference;

#[async_trait]
pub trait AbstractReport: Sync + Send {
    /// Insert a new report into the database
    async fn insert_report(&self, report: &Report) -> Result<()>;

    /// Fetch all reports, optionally filtered by status
    async fn fetch_reports(&self, status: Option<String>) -> Result<Vec<Report>>;

    /// Fetch a single report by ID
    async fn fetch_report(&self, id: &str) -> Result<Report>;

    /// Update a report's status
    async fn update_report_status(&self, id: &str, status: ReportStatus) -> Result<()>;

    /// Count reports with "Created" status (pending triage)
    async fn count_pending_reports(&self) -> Result<u64>;
}
