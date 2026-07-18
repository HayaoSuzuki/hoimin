fn main() {
    if let Some(exit_code) = hoimin_cli::resource::run_linux_launcher_from(std::env::args_os()) {
        std::process::exit(exit_code);
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build Tokio runtime");
    let exit_code = runtime.block_on(hoimin_cli::run_from(std::env::args_os()));
    std::process::exit(exit_code);
}
