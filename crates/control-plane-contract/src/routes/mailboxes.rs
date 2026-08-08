use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
        ["api", "v1", "tenants", _, "mailboxes"] if method == "POST" => {
            Some(RouteClass::MailboxBindingCollectionApi)
        }
        ["api", "v1", "tenants", _, "mailboxes", _, "revoke"] if method == "POST" => {
            Some(RouteClass::MailboxBindingRevokeApi)
        }
        ["api", "v1", "tenants", _, "mailboxes", _, "jobs"] if method == "POST" => {
            Some(RouteClass::MailboxJobCollectionApi)
        }
        ["api", "v1", "tenants", _, "mailboxes", _, "jobs", _, "run"] if method == "POST" => {
            Some(RouteClass::MailboxJobRunApi)
        }
        ["api", "v1", "tenants", _, "mailboxes", _, "jobs", _] if method == "GET" => {
            Some(RouteClass::MailboxJobResourceApi)
        }
        ["api", "v1", "tenants", _, "mailboxes", _] if method == "GET" => {
            Some(RouteClass::MailboxBindingResourceApi)
        }
        _ => None,
    }
}
