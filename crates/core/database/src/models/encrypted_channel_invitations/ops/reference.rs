use company_result::Result;

use crate::{EncryptedChannelInvitation, ReferenceDb};

use super::AbstractEncryptedChannelInvitations;

#[async_trait]
impl AbstractEncryptedChannelInvitations for ReferenceDb {
    /// Insert a new encrypted channel invitation
    async fn insert_encrypted_invitation(
        &self,
        invitation: &EncryptedChannelInvitation,
    ) -> Result<()> {
        let mut invitations = self.encrypted_channel_invitations.lock().await;
        if invitations.contains_key(&invitation.id) {
            Err(create_database_error!("insert", "encrypted_channel_invitations"))
        } else {
            invitations.insert(invitation.id.clone(), invitation.clone());
            Ok(())
        }
    }

    /// Fetch an encrypted invitation by channel_id and bot_id
    async fn fetch_encrypted_invitation(
        &self,
        channel_id: &str,
        bot_id: &str,
    ) -> Result<Option<EncryptedChannelInvitation>> {
        let invitations = self.encrypted_channel_invitations.lock().await;
        Ok(invitations
            .values()
            .find(|inv| inv.channel_id == channel_id && inv.bot_id == bot_id)
            .cloned())
    }

    /// Delete an encrypted invitation by its id
    async fn delete_encrypted_invitation(&self, id: &str) -> Result<()> {
        let mut invitations = self.encrypted_channel_invitations.lock().await;
        if invitations.remove(id).is_some() {
            Ok(())
        } else {
            Err(create_error!(NotFound))
        }
    }
}
