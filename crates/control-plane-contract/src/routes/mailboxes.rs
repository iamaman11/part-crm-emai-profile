use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
        ["api", "v1", "tenants", _, "mailbox-onboardings", _, "gmail-oauth"]
            if method == "POST" =>
        {
            Some(RouteClass::MailboxBindingResourceApi)
        }
        ["auth", "v1", "mailbox", "gmail", "callback"] if method == "GET" => {
            Some(RouteClass::MailboxBindingResourceApi)
        }
        ["api", "v1", "tenants", _, "mailboxes"] if matches!(method, "GET" | "POST") => {
            Some(RouteClass::MailboxBindingCollectionApi)
        }
        [
            "api",
            "v1",
            "tenants",
            _,
            "mailboxes",
            _,
            "client-association",
        ] if matches!(method, "GET" | "POST") => Some(RouteClass::MailboxBindingResourceApi),
        [
            "api",
            "v1",
            "tenants",
            _,
            "mailboxes",
            _,
            "browser-execution",
        ] if method == "POST" => Some(RouteClass::MailboxBrowserExecutionBindApi),
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

#[cfg(test)]
mod tests {
    use super::classify;
    use crate::RouteClass;

    #[test]
    fn association_subresource_uses_mailbox_resource_family_and_is_method_fail_closed() {
        let segments = [
            "api",
            "v1",
            "tenants",
            "tenant_01",
            "mailboxes",
            "mailbox_01",
            "client-association",
        ];
        assert_eq!(
            classify("GET", &segments),
            Some(RouteClass::MailboxBindingResourceApi)
        );
        assert_eq!(
            classify("POST", &segments),
            Some(RouteClass::MailboxBindingResourceApi)
        );
        for method in ["PUT", "PATCH", "DELETE"] {
            assert_eq!(classify(method, &segments), None);
        }
    }

    #[test]
    fn gmail_oauth_routes_are_exact_and_wrong_methods_fail_closed() {
        let start = [
            "api",
            "v1",
            "tenants",
            "tenant_01",
            "mailbox-onboardings",
            "onboarding_01",
            "gmail-oauth",
        ];
        let callback = ["auth", "v1", "mailbox", "gmail", "callback"];
        assert_eq!(
            classify("POST", &start),
            Some(RouteClass::MailboxBindingResourceApi)
        );
        assert_eq!(
            classify("GET", &callback),
            Some(RouteClass::MailboxBindingResourceApi)
        );
        for method in ["GET", "PUT", "PATCH", "DELETE"] {
            assert_eq!(classify(method, &start), None);
        }
        for method in ["POST", "PUT", "PATCH", "DELETE"] {
            assert_eq!(classify(method, &callback), None);
        }
    }
}
