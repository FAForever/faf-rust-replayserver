use faf_rust_replayserver::util::process::wait_for_signals;

#[tokio::main]
async fn main() {
    println!("Waiting for sigint");
    wait_for_signals().await;
    println!("Received a sigint");
}
