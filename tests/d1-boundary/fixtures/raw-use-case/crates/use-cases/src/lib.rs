use worker::d1::D1Database;

pub async fn unsafe_unscoped_lookup(database: D1Database) -> worker::Result<()> {
    database.prepare("SELECT * FROM clients").run().await?;
    Ok(())
}
