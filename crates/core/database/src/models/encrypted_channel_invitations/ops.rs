use company_result::Result;

use crate::EncryptedChannelInvitation;

#[cfg(feature = "mongodb")]
mod mongodb;
mod reference;

#[async_trait]
pub trait AbstractEncryptedChannelInvitations: Sync + Send {
    /// Insert a new encrypted channel invitation
    async fn insert_encrypted_invitation(
        &self,
        invitation: &EncryptedChannelInvitation,
    ) -> Result<()>;

    /// Fetch an encrypted invitation by channel_id and bot_id
    async fn fetch_encrypted_invitation(
        &self,
        channel_id: &str,
        bot_id: &str,
    ) -> Result<Option<EncryptedChannelInvitation>>;

    /// Delete an encrypted invitation by its id
    async fn delete_encrypted_invitation(&self, id: &str) -> Result<()>;
}
