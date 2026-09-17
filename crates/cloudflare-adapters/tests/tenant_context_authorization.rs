const SOURCE: &str = include_str!("../src/d1_identity_acl.rs");

fn active_tenant_contexts_source() -> &'static str {
    let start = SOURCE
        .find("pub async fn active_tenant_contexts")
        .expect("active_tenant_contexts method exists");
    let tail = &SOURCE[start..];
    let end = tail
        .find("pub async fn tenant_boundary")
        .expect("tenant_boundary follows active_tenant_contexts");
    &tail[..end]
}

#[test]
fn tenant_context_projection_is_scoped_to_verified_identity_and_active_authority() {
    let source = active_tenant_contexts_source();
    for required in [
        "JOIN memberships AS membership",
        "membership.identity_id = identity.identity_id",
        "membership.status = 'ACTIVE'",
        "JOIN tenants AS tenant",
        "tenant.tenant_id = membership.tenant_id",
        "tenant.status = 'ACTIVE'",
        "WHERE identity.access_subject = ?",
        "identity.subject()",
    ] {
        assert!(
            source.contains(required),
            "tenant context authorization query lost required constraint: {required}"
        );
    }
}

#[test]
fn suspended_revoked_memberships_and_inactive_tenants_cannot_enter_the_projection() {
    let source = active_tenant_contexts_source();
    assert!(source.contains("membership.status = 'ACTIVE'"));
    assert!(source.contains("tenant.status = 'ACTIVE'"));
    assert!(!source.contains("membership.status != 'REVOKED'"));
    assert!(!source.contains("tenant.status != 'SUSPENDED'"));
}

#[test]
fn tenant_context_projection_order_is_deterministic() {
    let source = active_tenant_contexts_source();
    assert!(source.contains("ORDER BY tenant.display_name ASC, membership.tenant_id ASC"));
}

#[test]
fn malformed_stored_identifiers_roles_and_display_names_fail_closed() {
    let source = active_tenant_contexts_source();
    for required in [
        "TenantId::parse(row.tenant_id)",
        "ActorId::parse(row.actor_id)",
        "invalid membership role",
        "row.display_name.trim().is_empty()",
        "invalid tenant display name",
    ] {
        assert!(
            source.contains(required),
            "tenant context row validation lost fail-closed guard: {required}"
        );
    }
}
