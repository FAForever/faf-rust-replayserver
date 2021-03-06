use faf_rust_replayserver::util::process::setup_process_exit_on_panic;

fn main() {
    setup_process_exit_on_panic();
    let _t = std::thread::spawn(|| {
        panic!("This panic should cause us to return with exit code 1");
    });
    std::thread::sleep(std::time::Duration::from_millis(100));
    std::process::exit(0);
}
