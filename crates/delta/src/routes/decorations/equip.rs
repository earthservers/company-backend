use company_database::{
    Database, DecorationEquip, DecorationStatus, PartialUser, User,
};
use company_result::{create_error, Result};
use rocket::serde::json::Json;
use rocket::State;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
pub struct EquipResponse {
    pub success: bool,
}

/// # Equip Decoration
///
/// Equip an approved decoration to your profile. Replaces any existing decoration in the same slot.
/// For paid decorations, the user must own the decoration (have purchased it).
#[openapi(tag = "Decorations")]
#[post("/<id>/equip")]
pub async fn equip_decoration(
    db: &State<Database>,
    mut user: User,
    id: String,
) -> Result<Json<EquipResponse>> {
    let decoration = db.fetch_decoration(&id).await?;

    // Must be approved
    if !matches!(decoration.status, DecorationStatus::Approved) {
        return Err(create_error!(InvalidOperation));
    }

    // For paid decorations, check ownership
    if !decoration.is_free && decoration.price_cents > 0 {
        let owns = db.user_owns_decoration(&user.id, &id).await?;
        if !owns {
            return Err(create_error!(FailedValidation {
                error: "You must purchase this decoration before equipping it".to_string()
            }));
        }
    }

    // Build new active_decorations list
    let new_equip = DecorationEquip {
        decoration_id: decoration.id.clone(),
        slot: decoration.category.clone(),
    };

    let mut decorations = user.active_decorations.clone().unwrap_or_default();

    // Check if already equipped in this slot, track if we need to decrement old one
    let old_decoration_id = decorations
        .iter()
        .find(|d| d.slot == decoration.category)
        .map(|d| d.decoration_id.clone());

    // Remove existing in same slot
    decorations.retain(|d| d.slot != decoration.category);
    decorations.push(new_equip);

    // Update user
    user.update(
        db,
        PartialUser {
            active_decorations: Some(decorations),
            ..Default::default()
        },
        vec![],
    )
    .await?;

    // Decrement old decoration counter if replacing
    if let Some(old_id) = &old_decoration_id {
        if old_id != &decoration.id {
            let _ = db
                .decrement_decoration_counter(old_id, "active_users_count")
                .await;
        }
    }

    // Increment new decoration counter (unless re-equipping same)
    if old_decoration_id.as_deref() != Some(&decoration.id) {
        db.increment_decoration_counter(&decoration.id, "active_users_count")
            .await?;
    }

    Ok(Json(EquipResponse { success: true }))
}

/// # Unequip Decoration
///
/// Remove a decoration from your profile.
#[openapi(tag = "Decorations")]
#[delete("/<id>/equip")]
pub async fn unequip_decoration(
    db: &State<Database>,
    mut user: User,
    id: String,
) -> Result<Json<EquipResponse>> {
    let mut decorations = user.active_decorations.clone().unwrap_or_default();

    // Find and remove the decoration
    let had_it = decorations.iter().any(|d| d.decoration_id == id);
    if !had_it {
        return Err(create_error!(NotFound));
    }

    decorations.retain(|d| d.decoration_id != id);

    // Update user
    user.update(
        db,
        PartialUser {
            active_decorations: Some(decorations),
            ..Default::default()
        },
        vec![],
    )
    .await?;

    // Decrement active users count
    let _ = db
        .decrement_decoration_counter(&id, "active_users_count")
        .await;

    Ok(Json(EquipResponse { success: true }))
}
