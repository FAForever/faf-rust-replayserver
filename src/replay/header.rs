use tokio::io::{AsyncBufRead, AsyncReadExt};

use crate::{server::connection::Connection, error::ConnResult, server::connection::read_until_exact};

pub struct ReplayHeader {
    pub data: Vec<u8>,
}

impl ReplayHeader {
    pub async fn from_connection(c: &mut Connection) -> ConnResult<Self> {
        let max_size = 1024 * 1024;
        let limited = c.take(max_size);
        Self::do_from_connection(limited).await.map_err(|x| x.into())
    }

    async fn skip<T: AsyncBufRead + Unpin>(r: &mut T, count: u64, buf: &mut Vec<u8>) -> std::io::Result<()> {
        let read = r.take(count).read_to_end(buf).await?;
        if read < count as usize {
            Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, format!("EOF reached before skipping {} bytes", count)))
        } else {
            Ok(())
        }
    }

    async fn do_from_connection<T: AsyncBufRead + Unpin>(mut r: T) -> std::io::Result<Self> {
        let mut data = Vec::<u8>::new();
        macro_rules! read_value {
            ($f: ident) => {
                {
                    let i = r.$f().await?;
                    data.extend_from_slice(&i.to_le_bytes());
                    i
                }
            }
        }

        let _version = read_until_exact(&mut r, 0, &mut data).await;
        Self::skip(&mut r, 3, &mut data).await?;
        let _replay_version_and_map = read_until_exact(&mut r, 0, &mut data).await;
        Self::skip(&mut r, 4, &mut data).await?;

        let mod_data_size = read_value!(read_u32_le);
        Self::skip(&mut r, mod_data_size as u64, &mut data).await?;

        let scenario_info_size = read_value!(read_u32_le);
        Self::skip(&mut r, scenario_info_size as u64, &mut data).await?;

        let player_count = r.read_u8().await?;
        data.push(player_count);
        for _ in 0..player_count {
            let _name = read_until_exact(&mut r, 0, &mut data).await;
            let _timeout_count = read_value!(read_u32_le);
        }

        let _cheats_enabled = read_value!(read_u8);

        let army_count = read_value!(read_u8);
        for _ in 0..army_count {
            let _army_size = read_value!(read_u32_le);
            Self::skip(&mut r, _army_size as u64, &mut data).await?;
            let player_id = read_value!(read_u8);
            if player_id != 255 {
                Self::skip(&mut r, 1, &mut data).await?;
            }
        }

        let _random_seed = read_value!(read_u32_le);
        Ok(ReplayHeader{data})
    }
}
