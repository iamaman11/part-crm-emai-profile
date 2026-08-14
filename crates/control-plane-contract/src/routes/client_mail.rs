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

#[cfg(test)]
mod tests {
    use super::classify;
    use crate::RouteClass;

    #[test]
    fn send_route_is_exact_and_post_only() {
        let segments = [
            "api",
            "v1",
            "tenants",
            "tenant_01",
            "clients",
            "client_01",
            "mail",
            "send",
        ];
        assert_eq!(classify("POST", &segments), Some(RouteClass::ClientMailSendApi));
        assert_eq!(classify("GET", &segments), None);

        let extra = [
            "api",
            "v1",
            "tenants",
            "tenant_01",
            "clients",
            "client_01",
            "mail",
            "send",
            "extra",
        ];
        assert_eq!(classify("POST", &extra), None);
    }
}
