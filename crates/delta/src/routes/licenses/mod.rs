use company_config::LicenseServer;
use company_database::User;
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ActivateLicenseRequest {
    pub license_key: String,
    pub hardware_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct ActivateLicenseResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activated_at: Option<String>,
}

/// POST /api/licenses/activate
///
/// Proxy license activation to the license server. Requires authenticated user.
#[post("/activate", data = "<request>")]
pub async fn activate_license(
    user: User,
    request: Json<ActivateLicenseRequest>,
    license_server: &State<LicenseServer>,
) -> Result<Json<ActivateLicenseResponse>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|_| create_error!(InternalError))?;

    let activate_url = format!("{}/api/activate", license_server.url);

    let body = serde_json::json!({
        "license_key": request.license_key,
        "hardware_id": request.hardware_id,
        "user_id": user.id,
    });

    let response = client
        .post(&activate_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            log::error!("License activation request failed: {e}");
            create_error!(InternalError)
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        log::warn!("License server returned {status}: {error_text}");
        return Ok(Json(ActivateLicenseResponse {
            success: false,
            message: format!("License server error: {status}"),
            license_type: None,
            user_id: None,
            expires_at: None,
            activated_at: None,
        }));
    }

    let data: ActivateLicenseResponse = response.json().await.map_err(|e| {
        log::error!("Failed to parse license server response: {e}");
        create_error!(InternalError)
    })?;

    Ok(Json(data))
}
