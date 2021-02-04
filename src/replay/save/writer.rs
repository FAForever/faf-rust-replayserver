use crate::replay::streams::MReplayRef;
use crate::replay::streams::MergedReplayReader;
use async_compression::tokio::write::ZstdEncoder;
use tokio::io::AsyncWriteExt;

pub async fn write_replay(
    mut to: tokio::fs::File,
    json_header: impl serde::Serialize,
    replay: MReplayRef,
) -> std::io::Result<()> {
    debug_assert!(replay.borrow().get_header().is_none());
    to.write_all(serde_json::to_string(&json_header)?.as_bytes())
        .await?;
    to.write_all("\n".as_bytes()).await?;
    let mut encoder = ZstdEncoder::with_quality(to, async_compression::Level::Precise(10));
    let mut replay_writer = MergedReplayReader::new(replay);
    replay_writer.write_to(&mut encoder).await?;
    Ok(())
}
