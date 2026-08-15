use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
        ["api", "v1", "tenants", _, "clients", _, "mail", "search"] if method == "POST" => {
            Some(RouteClass::ClientMailSearchApi)
        }
        ["api", "v1", "tenants", _, "clients", _, "mail", "message"] if method == "POST" => {
            Some(RouteClass::ClientMailMessageApi)
        }
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

    fn route_segments(action: &'static str) -> [&'static str; 8] {
        [
            "api",
            "v1",
            "tenants",
            "tenant_01",
            "clients",
            "client_01",
            "mail",
            action,
        ]
    }

    #[test]
    fn client_mail_routes_are_exact_and_post_only() {
        for (action, expected) in [
            ("search", RouteClass::ClientMailSearchApi),
            ("message", RouteClass::ClientMailMessageApi),
            ("send", RouteClass::ClientMailSendApi),
        ] {
            let segments = route_segments(action);
            assert_eq!(classify("POST", &segments), Some(expected));
            assert_eq!(classify("GET", &segments), None);
            assert_eq!(classify("PUT", &segments), None);

            let extra = [
                "api",
                "v1",
                "tenants",
                "tenant_01",
                "clients",
                "client_01",
                "mail",
                action,
                "extra",
            ];
            assert_eq!(classify("POST", &extra), None);
        }
    }
}
