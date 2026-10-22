fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let output = match arguments.as_slice() {
        [] => control_plane_contract::public_api::openapi_json_pretty()?,
        [kind] if kind == "device-registration" => serde_json::to_string_pretty(
            &control_plane_contract::device_application_api::registration_openapi_fragment(),
        )?,
        [kind] if kind == "profile-launch" => serde_json::to_string_pretty(
            &control_plane_contract::profile_launch_api::openapi_fragment(),
        )?,
        _ => return Err("unsupported OpenAPI export".into()),
    };
    print!("{output}");
    Ok(())
}
