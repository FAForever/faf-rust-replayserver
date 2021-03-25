use faf_rust_replayserver::util::process::wait_for_signals;

#[tokio::main]
async fn main() {
    wait_for_signals().await;
    println!("Received a sigint");
}
