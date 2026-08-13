#[path = "../client_mail_send_schema_common.rs"]
mod client_mail_send_schema_common;
#[path = "../client_mail_send_schema_request.rs"]
mod client_mail_send_schema_request;
#[path = "../client_mail_send_schema.rs"]
mod client_mail_send_schema;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string(&client_mail_send_schema::openapi_fragment())?);
    Ok(())
}
