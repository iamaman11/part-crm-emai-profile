fn main() -> Result<(), Box<dyn std::error::Error>> {
    print!(
        "{}",
        control_plane_contract::public_api::openapi_json_pretty()?
    );
    Ok(())
}
