#[tokio::main]
async fn main() -> anyhow::Result<()> {
    lan_save_sync_core::run("agent").await
}
