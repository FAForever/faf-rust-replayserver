use tokio::io::{AsyncBufRead, AsyncReadExt};

use crate::{error::ConnResult, server::connection::read_until_exact};

const MAX_SIZE: u64 = 1024 * 1024;

pub struct ReplayHeader {
    pub data: Vec<u8>,
}

// Only show the very start / end
impl std::fmt::Debug for ReplayHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let start = &self.data[..std::cmp::min(30, self.data.len())];
        let end = &self.data[self.data.len().saturating_sub(30)..];
        f.debug_struct("ReplayHeader")
            .field("data (start)", &format_args!("{:?}", &start))
            .field("data (end)", &format_args!("{:?}", &end))
            .finish()
    }
}

impl ReplayHeader {
    pub async fn from_connection<T: AsyncBufRead + Unpin>(c: &mut T) -> ConnResult<Self> {
        let limited = c.take(MAX_SIZE);
        Self::do_from_connection(limited)
            .await
            .map_err(|x| x.into())
    }

    async fn skip<T: AsyncBufRead + Unpin>(
        r: &mut T,
        count: u64,
        buf: &mut Vec<u8>,
    ) -> std::io::Result<()> {
        let read = r.take(count).read_to_end(buf).await?;
        if read < count as usize {
            Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                format!("EOF reached before skipping {} bytes", count),
            ))
        } else {
            Ok(())
        }
    }

    async fn do_from_connection<T: AsyncBufRead + Unpin>(mut r: T) -> std::io::Result<Self> {
        let mut data = Vec::<u8>::new();
        macro_rules! read_value {
            ($f: ident) => {{
                let i = r.$f().await?;
                data.extend_from_slice(&i.to_le_bytes());
                i
            }};
        }

        let _version = read_until_exact(&mut r, 0, &mut data).await?;
        Self::skip(&mut r, 3, &mut data).await?;
        let _replay_version_and_map = read_until_exact(&mut r, 0, &mut data).await?;
        Self::skip(&mut r, 4, &mut data).await?;

        let mod_data_size = read_value!(read_u32_le);
        Self::skip(&mut r, mod_data_size as u64, &mut data).await?;

        let scenario_info_size = read_value!(read_u32_le);
        Self::skip(&mut r, scenario_info_size as u64, &mut data).await?;

        let player_count = r.read_u8().await?;
        data.push(player_count);
        for _ in 0..player_count {
            let _name = read_until_exact(&mut r, 0, &mut data).await?;
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
        Ok(ReplayHeader { data })
    }
}

#[cfg(test)]
mod test {
    use std::{fs::File, io::Read, path::PathBuf};

    use futures::Future;
    use tokio::{
        io::DuplexStream,
        io::{AsyncWriteExt, BufReader},
        try_join,
    };

    use super::*;

    fn get_file(f: &str) -> Vec<u8> {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("test/resources");
        p.push(f);
        let mut res = Vec::new();
        File::open(p).unwrap().read_to_end(&mut res).unwrap();
        res
    }

    async fn test_replay_on_data<F: Future<Output = ()>, FN>(data: &Vec<u8>, test: FN)
    where
        FN: Fn(BufReader<DuplexStream>) -> F,
    {
        let (mut i, o) = tokio::io::duplex(1024);
        let buf_read = BufReader::new(o);

        let writing_data = async {
            i.write_all(data.as_ref()).await.unwrap();
            drop(i);
        };
        let read_header = test(buf_read);

        // Return when test ends, but not when data writing ends
        let _: Result<((), ()), ()> = try_join! {
            async { writing_data.await ; Ok(()) },
            async {read_header.await ; Err(()) },
        };
    }

    #[tokio::test]
    async fn example_header() {
        let example_header = get_file("example_header");
        let eref = &example_header;

        test_replay_on_data(&example_header, |mut buf_read| async move {
            let example_header = eref;
            let header = ReplayHeader::from_connection(&mut buf_read).await.unwrap();
            let expected: &[u8] = example_header.as_ref();
            assert_eq!(header.data, expected);
        })
        .await;
    }

    #[tokio::test]
    async fn short_header() {
        let example_header = get_file("example_header");

        // Header has ~1600 bytes, so it's okay to test all lengths.
        for i in 0..example_header.len() {
            let mut short_header = example_header.clone();
            short_header.truncate(i);
            test_replay_on_data(&short_header, |mut buf_read| async move {
                ReplayHeader::from_connection(&mut buf_read)
                    .await
                    .expect_err("Reading short header should result in an error!");
            })
            .await;
        }
    }

    #[tokio::test]
    async fn too_large_header() {
        let example_header = get_file("example_header");

        // We start with reading until \0, so let's prepend a lot of 1's
        let mut long_header = vec![];
        for _ in 0..(MAX_SIZE - 500) {
            long_header.push(1);
        }
        long_header.append(&mut example_header.clone());
        test_replay_on_data(&long_header, |mut buf_read| async move {
            ReplayHeader::from_connection(&mut buf_read)
                .await
                .expect_err("Header should be too long");
        })
        .await;

        // Just to be sure, check if a not-quite-long enough header works fine
        let mut short_header = vec![];
        for _ in 0..(MAX_SIZE - 2000) {
            short_header.push(1);
        }
        short_header.append(&mut example_header.clone());
        test_replay_on_data(&short_header, |mut buf_read| async move {
            ReplayHeader::from_connection(&mut buf_read)
                .await
                .expect("Header should be short enough");
        })
        .await;
    }
}
