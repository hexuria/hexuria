use axum::http::request::Parts;
use leptos::prelude::*;
use payplan_app::queries::{PageRequest, TenantScope};
use payplan_core::platform::user::UserRole;
use payplan_web::session::AuthUser;
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub(crate) struct PageQuery {
    pub(crate) page: Option<u32>,
    pub(crate) page_size: Option<u32>,
    pub(crate) query: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) next: Option<String>,
}

pub(crate) fn page_request() -> PageRequest {
    let query = request_query();
    PageRequest {
        page: query.page.unwrap_or(1),
        page_size: query.page_size.unwrap_or(25),
        query: query.query,
    }
    .normalized()
}

pub(crate) fn request_query() -> PageQuery {
    use_context::<Parts>()
        .and_then(|parts| parts.uri.query().map(str::to_string))
        .and_then(|query| serde_urlencoded::from_str(&query).ok())
        .unwrap_or_default()
}

pub(crate) fn current_user() -> Option<AuthUser> {
    use_context::<Parts>().and_then(|parts| parts.extensions.get::<AuthUser>().cloned())
}

pub(crate) fn scope(auth: &AuthUser) -> TenantScope {
    if auth.role == UserRole::PlatformAdmin {
        TenantScope::Platform
    } else {
        auth.company_id
            .map(TenantScope::Company)
            .unwrap_or(TenantScope::Platform)
    }
}
