use leptos::prelude::*;
use payplan_core::platform::user::UserRole;

use crate::{
    app::current_user,
    components::{Forbidden, JobForm, LoginRequired, PageFrame},
};

#[component]
pub(crate) fn JobsPage() -> impl IntoView {
    let Some(auth) = current_user() else {
        return view! { <LoginRequired/> }.into_any();
    };
    if auth.role != UserRole::PlatformAdmin {
        return view! { <Forbidden/> }.into_any();
    }
    view! {
        <PageFrame title="Operations jobs">
            <div class="job-grid">
                <JobForm action="/jobs/renewals" label="Run renewals"/>
                <JobForm action="/jobs/royal-pot" label="Distribute Royal pot"/>
                <JobForm action="/jobs/binary-cycle" label="Close binary cycle"/>
            </div>
        </PageFrame>
    }
    .into_any()
}
