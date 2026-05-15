use iso8601_timestamp::Timestamp;
use company_database::{
    util::reference::Reference, AuditAction, AuditLogEntry, Database, User,
};
use company_models::v0::ReportStatus;
use company_result::{create_error, Result};
use rocket::{serde::json::Json, State};
use rocket_empty::EmptyResponse;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// # Report Response
#[derive(Serialize, JsonSchema)]
pub struct ReportResponse {
    pub id: String,
    pub author_id: String,
    pub content: company_models::v0::ReportedContent,
    pub additional_context: String,
    #[serde(flatten)]
    pub status: company_models::v0::ReportStatus,
    pub notes: String,
}

impl From<company_database::Report> for ReportResponse {
    fn from(r: company_database::Report) -> Self {
        Self {
            id: r.id,
            author_id: r.author_id,
            content: r.content,
            additional_context: r.additional_context,
            status: r.status,
            notes: r.notes,
        }
    }
}

/// # List Reports
///
/// Fetch all reports. Privileged users only.
/// Optionally filter by status: "Created", "Resolved", or "Rejected".
#[openapi(tag = "Admin")]
#[get("/reports?<status>")]
pub async fn list_reports(
    db: &State<Database>,
    user: User,
    status: Option<String>,
) -> Result<Json<Vec<ReportResponse>>> {
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }

    let reports = db.fetch_reports(status).await?;
    Ok(Json(reports.into_iter().map(Into::into).collect()))
}

/// # Resolve Report Data
#[derive(Deserialize, JsonSchema)]
pub struct DataResolveReport {
    /// Optional notes about the resolution
    #[serde(default)]
    pub notes: Option<String>,
}

/// # Resolve Report
///
/// Mark a report as resolved. Privileged users only.
#[openapi(tag = "Admin")]
#[patch("/reports/<target>/resolve", data = "<data>")]
pub async fn resolve_report(
    db: &State<Database>,
    user: User,
    target: Reference<'_>,
    data: Json<DataResolveReport>,
) -> Result<EmptyResponse> {
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }

    let report = db.fetch_report(target.id).await?;

    db.update_report_status(
        &report.id,
        ReportStatus::Resolved {
            closed_at: Some(Timestamp::now_utc()),
        },
    )
    .await?;

    // Write audit log
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    db.insert_audit_log(&AuditLogEntry {
        id: Ulid::new().to_string(),
        actor_id: user.id,
        action: AuditAction::ResolveReport,
        target_id: report.id,
        timestamp: now,
        reason: data.into_inner().notes,
    })
    .await?;

    Ok(EmptyResponse)
}

/// # Reject Report Data
#[derive(Deserialize, JsonSchema)]
pub struct DataRejectReport {
    /// Reason for rejecting the report
    pub rejection_reason: String,
}

/// # Reject Report
///
/// Reject a report. Privileged users only.
#[openapi(tag = "Admin")]
#[patch("/reports/<target>/reject", data = "<data>")]
pub async fn reject_report(
    db: &State<Database>,
    user: User,
    target: Reference<'_>,
    data: Json<DataRejectReport>,
) -> Result<EmptyResponse> {
    if !user.privileged {
        return Err(create_error!(NotPrivileged));
    }

    let data = data.into_inner();
    let report = db.fetch_report(target.id).await?;

    db.update_report_status(
        &report.id,
        ReportStatus::Rejected {
            rejection_reason: data.rejection_reason.clone(),
            closed_at: Some(Timestamp::now_utc()),
        },
    )
    .await?;

    // Write audit log
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    db.insert_audit_log(&AuditLogEntry {
        id: Ulid::new().to_string(),
        actor_id: user.id,
        action: AuditAction::RejectReport,
        target_id: report.id,
        timestamp: now,
        reason: Some(data.rejection_reason),
    })
    .await?;

    Ok(EmptyResponse)
}
