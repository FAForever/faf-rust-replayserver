use faf_rust_replayserver::util::process::wait_for_sigint;

#[tokio::main]
async fn main() {
    wait_for_sigint().await;
    println!("Received a sigint");
}
