#[tokio::main]
async fn main() {
    let exit_code = hoimin_cli::run_from(std::env::args_os()).await;
    std::process::exit(exit_code);
}
