use company_result::Result;

use crate::{EncryptedChannelInvitation, MongoDb};

use super::AbstractEncryptedChannelInvitations;

static COL: &str = "encrypted_channel_invitations";

#[async_trait]
impl AbstractEncryptedChannelInvitations for MongoDb {
    /// Insert a new encrypted channel invitation
    async fn insert_encrypted_invitation(
        &self,
        invitation: &EncryptedChannelInvitation,
    ) -> Result<()> {
        query!(self, insert_one, COL, &invitation).map(|_| ())
    }

    /// Fetch an encrypted invitation by channel_id and bot_id
    async fn fetch_encrypted_invitation(
        &self,
        channel_id: &str,
        bot_id: &str,
    ) -> Result<Option<EncryptedChannelInvitation>> {
        query!(
            self,
            find_one,
            COL,
            doc! {
                "channel_id": channel_id,
                "bot_id": bot_id
            }
        )
    }

    /// Delete an encrypted invitation by its id
    async fn delete_encrypted_invitation(&self, id: &str) -> Result<()> {
        query!(self, delete_one_by_id, COL, id).map(|_| ())
    }
}
