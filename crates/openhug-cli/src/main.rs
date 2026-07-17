#[tokio::main]
async fn main() -> anyhow::Result<()> {
    openhug_cli::run().await
}
