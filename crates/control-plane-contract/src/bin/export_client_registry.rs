use control_plane_contract::{client_registry_api, public_api};

#[path = "../client_registry_fragment.rs"]
mod client_registry_fragment;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "canonical".to_owned());
    let document = match mode.as_str() {
        "canonical" => client_registry_fragment::canonical_fragment()?,
        "compatibility" => client_registry_fragment::compatibility_fragment()?,
        _ => return Err(format!("unknown client registry export mode: {mode}").into()),
    };
    let mut rendered = serde_json::to_string_pretty(&document)?;
    rendered.push('\n');
    print!("{rendered}");
    Ok(())
}
