#![forbid(unsafe_code)]

use profile_platform_primitives::{TenantId, TenantScope};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tenant_id = TenantId::parse("tenant_foundation")?;
    let scope = TenantScope::new(tenant_id);

    println!("foundation-ready tenant_scope={}", scope.tenant_id());
    Ok(())
}
