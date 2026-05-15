use company_database::{
    util::{permissions::DatabasePermissionQuery, reference::Reference},
    Database, User,
};
use company_models::v0;

use company_permissions::{calculate_user_permissions, UserPermission};
use company_result::{create_error, Result};
use revolt_rocket_okapi::{
    gen::OpenApiGenerator,
    request::{OpenApiFromRequest, RequestHeaderInput},
};
use rocket::request::{FromRequest, Outcome, Request};
use rocket::{serde::json::Json, State};

use crate::util::external_auth::ServiceIdentity;

/// Either an authenticated Company user or a verified service identity.
///
/// Routes that take this guard accept both `x-session-token`-authed users and
/// service tokens (`Authorization: Bearer <jwt>` with aud=`company-internal`).
/// Service-token requests skip user-side permission checks but must carry the
/// required scope (checked at the handler level).
pub enum UserOrService {
    User(Box<User>),
    Service(ServiceIdentity),
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for UserOrService {
    type Error = authifier::Error;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        // Try service-token path first. The ServiceIdentity guard reads the
        // Authorization header; user sessions use `x-session-token`, so the
        // two never collide.
        if let Outcome::Success(svc) = request.guard::<ServiceIdentity>().await {
            return Outcome::Success(UserOrService::Service(svc));
        }
        match request.guard::<User>().await {
            Outcome::Success(u) => Outcome::Success(UserOrService::User(Box::new(u))),
            Outcome::Error((status, err)) => Outcome::Error((status, err)),
            Outcome::Forward(status) => Outcome::Forward(status),
        }
    }
}

// Mirrors the User schema — fetch_user is part of the public OpenAPI spec, so
// the composite guard needs to advertise the same security scheme.
impl OpenApiFromRequest<'_> for UserOrService {
    fn from_request_input(
        _gen: &mut OpenApiGenerator,
        _name: String,
        _required: bool,
    ) -> revolt_rocket_okapi::Result<RequestHeaderInput> {
        use revolt_rocket_okapi::revolt_okapi::openapi3::{SecurityScheme, SecuritySchemeData};
        let mut requirements = schemars::Map::new();
        requirements.insert("Session Token".to_owned(), vec![]);
        Ok(RequestHeaderInput::Security(
            "Session Token".to_owned(),
            SecurityScheme {
                data: SecuritySchemeData::ApiKey {
                    name: "x-session-token".to_owned(),
                    location: "header".to_owned(),
                },
                description: Some(
                    "User session OR an Authorization Bearer service-token \
                     (aud=company-internal, scope users:read)."
                        .to_owned(),
                ),
                extensions: schemars::Map::new(),
            },
            requirements,
        ))
    }
}

/// # Fetch User
///
/// Retrieve a user's information.
///
/// Accepts either:
///   * A logged-in Company user (`x-session-token`) — permission-filtered view.
///   * A service token (`Authorization: Bearer …`, aud=`company-internal`,
///     scope contains `users:read`) — unfiltered public view, used by
///     EarthSocial and other Earth Servers services for mirror hydration.
#[openapi(tag = "User Information")]
#[get("/<target>")]
pub async fn fetch(
    db: &State<Database>,
    auth: UserOrService,
    target: Reference<'_>,
) -> Result<Json<v0::User>> {
    match auth {
        UserOrService::User(user) => {
            if user.id == target.id {
                return Ok(Json(user.into_self(false).await));
            }

            let target = target.as_user(db).await?;

            let mut query = DatabasePermissionQuery::new(db, &*user).user(&target);
            calculate_user_permissions(&mut query)
                .await
                .throw_if_lacking_user_permission(UserPermission::Access)?;

            Ok(Json(target.into(db, &*user).await))
        }
        UserOrService::Service(svc) => {
            if !svc.has_scope("users:read") {
                return Err(create_error!(MissingPermission {
                    permission: "users:read".to_owned()
                }));
            }
            let target = target.as_user(db).await?;
            // Use the target as its own viewer so the result is the full
            // public-safe view with no relationship/permission filtering.
            // The shape matches what user-authed callers see, so consumers
            // (EarthSocial) can deserialise both paths with one client.
            let target_clone = target.clone();
            Ok(Json(target.into(db, &target_clone).await))
        }
    }
}
