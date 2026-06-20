use utoipa::Modify;
use utoipa::OpenApi;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};

use crate::constants::SESSION_NAME;

struct SessionCookie;

impl Modify for SessionCookie {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "session",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new(SESSION_NAME))),
        );
    }
}

/// Aggregated OpenAPI document for the kash-server REST API.
/// NOTE: paths are listed manually below; when adding/removing a route in
/// src/main.rs, update this list too (single small place, intentional tradeoff).
#[derive(OpenApi)]
#[openapi(
    info(title = "kash-server", version = env!("CARGO_PKG_VERSION")),
    servers((url = "/api")),
    modifiers(&SessionCookie),
    security(("session" = [])),
    paths(
        crate::auth::register,
        crate::auth::login,
        crate::auth::me,
        crate::auth::logout,
        crate::settings::get_settings,
        crate::settings::update_settings,
        crate::fx::get_fx_rates,
        crate::records::create_record,
        crate::records::get_records,
        crate::records::update_record,
        crate::records::delete_record,
        crate::categories::create_category,
        crate::categories::get_categories,
        crate::categories::update_category,
        crate::categories::delete_category,
        crate::friends::send_friend_request,
        crate::friends::search_users,
        crate::friends::update_nickname,
        crate::friends::list_friends,
        crate::friends::accept_friend,
        crate::friends::remove_friend,
        crate::splits::create_split,
        crate::splits::list_pending_shares,
        crate::splits::list_unsettled_shares,
        crate::splits::finalize_share,
        crate::splits::settle_share,
        crate::splits::settle_all_with_friend,
    )
)]
pub struct ApiDoc;
