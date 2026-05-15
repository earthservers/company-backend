auto_derived!(
    /// Admin audit log entry
    pub struct AuditLogEntry {
        /// Unique Id
        #[serde(rename = "_id")]
        pub id: String,
        /// Id of the admin who performed the action
        pub actor_id: String,
        /// What action was performed
        pub action: AuditAction,
        /// Id of the target user/report/object
        pub target_id: String,
        /// Unix timestamp of when the action was performed
        pub timestamp: i64,
        /// Optional reason or notes
        #[serde(skip_serializing_if = "Option::is_none")]
        pub reason: Option<String>,
    }

    /// Actions that can be performed by admins
    pub enum AuditAction {
        /// Banned a user from the platform
        BanUser,
        /// Unsuspended a user
        UnsuspendUser,
        /// Resolved a report
        ResolveReport,
        /// Rejected a report
        RejectReport,
        /// Granted premium to a user
        GrantPremium,
        /// Revoked premium from a user
        RevokePremium,
    }
);
