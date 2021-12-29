pub mod buf_deque;
pub mod buf_traits;
pub mod empty_counter;
pub mod event;
pub mod process;
pub mod test;
pub mod timeout;


pub fn pretty_bytes(bytes: &[u8]) -> String {
    return String::from_utf8(bytes.iter().flat_map(|x| std::ascii::escape_default(*x)).collect()).unwrap();
}
