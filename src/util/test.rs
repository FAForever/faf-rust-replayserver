use std::{fs::File, io::Read, path::PathBuf, sync::Once};

static LOG: Once = Once::new();

pub fn setup_logging() {
    LOG.call_once(env_logger::init);
}

pub fn get_file(f: &str) -> Vec<u8> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("test/resources");
    p.push(f);
    let mut res = Vec::new();
    File::open(p).unwrap().read_to_end(&mut res).unwrap();
    res
}
