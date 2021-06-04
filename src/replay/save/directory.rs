use std::path::PathBuf;

use tokio::io::AsyncWrite;

#[cfg_attr(test, faux::create)]
pub struct SavedReplayDirectory {
    root: PathBuf,
}

#[cfg_attr(test, faux::methods)]
impl SavedReplayDirectory {
    pub fn new(root: &str) -> Self {
        Self {
            root: PathBuf::from(root),
        }
    }

    fn replay_path(&self, replay_id: u64) -> PathBuf {
        // Legacy folder structure:
        // digits 3-10 from the right,
        let padded_id = format!("{:0>10}", replay_id.to_string());
        let digits = &padded_id[padded_id.len() - 10..padded_id.len() - 2];
        // in 4 groups by 2 starting by most significant,
        let groups = vec![&digits[0..2], &digits[2..4], &digits[4..6], &digits[6..8]];
        // NOT left-padded, so 0x -> x
        let dirs: Vec<String> = groups
            .into_iter()
            .map(|x| x.parse::<i32>().unwrap().to_string())
            .collect();

        dirs.into_iter().fold(self.root.clone(), |mut p, d| {
            p.push(d);
            p
        })
    }

    // Boxing so faux can work.
    pub async fn touch_and_return_file(&self, replay_id: u64) -> std::io::Result<Box<dyn AsyncWrite + Unpin>> {
        let mut target = self.replay_path(replay_id);
        tokio::fs::create_dir_all(&target).await?;

        let basename = format!("{}.fafreplay", replay_id);
        target.push(basename);
        Ok(Box::new(
            tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)
                .await?,
        ))
    }
}

#[cfg(test)]
pub mod test {
    use tokio::io::sink;

    use super::*;
    #[test]
    fn test_replay_path_works_as_intended() {
        let dir = SavedReplayDirectory::new("./");
        let path_1 = dir.replay_path(1234567);
        assert_eq!(path_1, PathBuf::from("./0/1/23/45"));

        let path_2 = dir.replay_path(1);
        assert_eq!(path_2, PathBuf::from("./0/0/0/0"));

        let path_3 = dir.replay_path(5500550055);
        assert_eq!(path_3, PathBuf::from("./55/0/55/0"));
    }

    #[test]
    fn test_replay_path_uses_provided_directory() {
        let dir = SavedReplayDirectory::new("/tmp/foo");
        let path_1 = dir.replay_path(1234567);
        assert_eq!(path_1, PathBuf::from("/tmp/foo/0/1/23/45"));
    }

    pub fn test_directory() -> SavedReplayDirectory {
        let mut f = SavedReplayDirectory::faux();
        faux::when!(f.touch_and_return_file).then(|_| Ok(Box::new(sink())));
        f
    }
}
