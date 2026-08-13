use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
        ["api", "v1", "tenants", _, "clients", _, "mail", "send"] if method == "POST" => {
            Some(RouteClass::ClientMailSendApi)
        }
        _ => None,
    }
}
