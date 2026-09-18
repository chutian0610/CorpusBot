#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind = std::env::var("CORPUSBOT_LOCAL_BIND")
        .unwrap_or_else(|_| "127.0.0.1:1421".to_owned())
        .parse()?;
    corpusbot_lib::local_server::serve(bind).await?;
    Ok(())
}
