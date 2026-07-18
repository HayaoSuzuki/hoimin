use std::ffi::OsString;

pub mod cli;

pub async fn run_from<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match cli::parse_from(args) {
        Ok(_) => 0,
        Err(cli::CliError::Clap(error)) => {
            let exit_code = error.exit_code();
            let _ = error.print();
            exit_code
        }
        Err(error) => {
            eprintln!("{error}");
            2
        }
    }
}
