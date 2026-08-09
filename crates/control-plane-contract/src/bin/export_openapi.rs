fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut document = control_plane_contract::public_api::openapi_document();
    control_plane_contract::client_registry_api::extend_openapi(&mut document);
    let mut rendered = serde_json::to_string_pretty(&document)?;
    rendered.push('\n');
    print!("{rendered}");
    Ok(())
}
