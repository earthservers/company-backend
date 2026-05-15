auto_derived!(
    /// Category of profile decoration
    pub enum DecorationCategory {
        AvatarFrame,
        Banner,
        Badge,
        Nameplate,
        ChatBubble,
        CardSmall,
        CardLarge,
        UserPopout,
        UserPopoutMobile,
        ChatBackground,
        ChatBackgroundMobile,
        ProfileModal,
    }

    /// A decoration equipped on a user's profile
    pub struct DecorationEquip {
        /// ID of the equipped decoration
        pub decoration_id: String,
        /// Which slot this decoration occupies
        pub slot: DecorationCategory,
    }
);
