use crate::RouteClass;

#[must_use]
pub(super) fn classify(method: &str, segments: &[&str]) -> Option<RouteClass> {
    match segments {
        ["api", "v1", "tenants", _, "owner", "bootstrap"] if method == "POST" => {
            Some(RouteClass::OwnerBootstrapApi)
        }
        ["api", "v1", "tenants", _, "owner", "transfer"] if method == "POST" => {
            Some(RouteClass::OwnerTransferApi)
        }
        ["api", "v1", "tenants", _, "invitations"] if method == "POST" => {
            Some(RouteClass::InvitationCollectionApi)
        }
        ["api", "v1", "tenants", _, "invitations", _, "accept"] if method == "POST" => {
            Some(RouteClass::InvitationAcceptApi)
        }
        ["api", "v1", "tenants", _, "members", _, "status"] if method == "PUT" => {
            Some(RouteClass::MembershipStatusApi)
        }
        _ => None,
    }
}
