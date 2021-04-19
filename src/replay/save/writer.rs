use crate::replay::streams::MReplayRef;
use crate::replay::streams::MergedReplayReader;
use async_compression::tokio::write::ZstdEncoder;
use tokio::io::{AsyncWrite, AsyncWriteExt};

pub async fn write_replay(
    mut to: impl AsyncWrite + Unpin,
    json_header: impl serde::Serialize,
    replay: MReplayRef,
    compression_level: u32,
) -> std::io::Result<()> {
    to.write_all(serde_json::to_string(&json_header)?.as_bytes()).await?;
    to.write_all("\n".as_bytes()).await?;
    let clevel = async_compression::Level::Precise(compression_level);
    let mut encoder = ZstdEncoder::with_quality(to, clevel);
    let mut replay_writer = MergedReplayReader::new(replay);
    replay_writer.write_to(&mut encoder).await?;
    encoder.shutdown().await?;
    Ok(())
}

#[cfg(test)]
pub mod test {
    use async_compression::tokio::bufread::ZstdDecoder;
    use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};

    pub async fn unpack_replay(from: impl AsyncRead + Unpin) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
        let mut read = BufReader::new(from);
        let mut json = Vec::new();
        let mut unc_replay = Vec::new();
        let mut replay = Vec::new();

        read.read_until(b'\n', &mut json).await?;
        read.read_to_end(&mut unc_replay).await?;
        let mut decoder = ZstdDecoder::new(&unc_replay[..]);
        decoder.read_to_end(&mut replay).await?;
        Ok((json, replay))
    }
}
