use reqwest::Client;
use serde::Deserialize;

/// Response from the license verification server
#[derive(Debug, Deserialize, Clone)]
pub struct LicenseStatus {
    /// Whether the license is valid
    pub valid: bool,
    /// License type: "cloud_basic", "cloud_pro", "cloud_elite", "download"
    pub license_type: String,
    /// User ID the license belongs to
    pub user_id: String,
    /// Expiration date for cloud licenses (ISO datetime), None for download
    pub expires_at: Option<String>,
}

/// Service for verifying AI Companion licenses
pub struct LicenseVerifier {
    license_server_url: String,
    http_client: Client,
}

/// Valid AI Companion license types
const VALID_LICENSE_TYPES: &[&str] = &[
    "cloud_basic",
    "cloud_pro",
    "cloud_elite",
    "download",
];

impl LicenseVerifier {
    pub fn new(license_server_url: String) -> Self {
        Self {
            license_server_url,
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to build HTTP client for license verification"),
        }
    }

    /// Verify that a user has a valid AI Companion license (cloud OR download)
    pub async fn verify_license(&self, user_id: &str) -> Result<LicenseStatus, String> {
        let url = format!(
            "{}/api/verify-license/{}",
            self.license_server_url, user_id
        );

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Unable to reach license server: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            return Err(format!("License server returned status {}", status));
        }

        let license = response
            .json::<LicenseStatus>()
            .await
            .map_err(|e| format!("Invalid response from license server: {}", e))?;

        if !license.valid {
            return Err("License is not active".to_string());
        }

        if !Self::is_valid_license_type(&license.license_type) {
            return Err(format!(
                "License type '{}' is not valid for AI Companion",
                license.license_type
            ));
        }

        Ok(license)
    }

    /// Check if a license type is valid for AI Companion
    pub fn is_valid_license_type(license_type: &str) -> bool {
        VALID_LICENSE_TYPES.contains(&license_type)
    }
}
