use company_result::Result;

use company_models::v0::ReportStatus;

use crate::ReferenceDb;
use crate::Report;

use super::AbstractReport;

#[async_trait]
impl AbstractReport for ReferenceDb {
    /// Insert a new report into the database
    async fn insert_report(&self, report: &Report) -> Result<()> {
        let mut reports = self.safety_reports.lock().await;
        if reports.contains_key(&report.id) {
            Err(create_database_error!("insert", "report"))
        } else {
            reports.insert(report.id.to_string(), report.clone());
            Ok(())
        }
    }

    async fn fetch_reports(&self, status: Option<String>) -> Result<Vec<Report>> {
        let reports = self.safety_reports.lock().await;
        Ok(reports
            .values()
            .filter(|r| {
                if let Some(ref status) = status {
                    match (&r.status, status.as_str()) {
                        (ReportStatus::Created {}, "Created") => true,
                        (ReportStatus::Rejected { .. }, "Rejected") => true,
                        (ReportStatus::Resolved { .. }, "Resolved") => true,
                        _ => false,
                    }
                } else {
                    true
                }
            })
            .cloned()
            .collect())
    }

    async fn fetch_report(&self, id: &str) -> Result<Report> {
        let reports = self.safety_reports.lock().await;
        reports
            .get(id)
            .cloned()
            .ok_or_else(|| create_error!(NotFound))
    }

    async fn update_report_status(&self, id: &str, status: ReportStatus) -> Result<()> {
        let mut reports = self.safety_reports.lock().await;
        if let Some(report) = reports.get_mut(id) {
            report.status = status;
            Ok(())
        } else {
            Err(create_error!(NotFound))
        }
    }

    async fn count_pending_reports(&self) -> Result<u64> {
        let reports = self.safety_reports.lock().await;
        Ok(reports
            .values()
            .filter(|r| matches!(r.status, ReportStatus::Created {}))
            .count() as u64)
    }
}
