use std::ffi::OsString;

pub mod analyzer;
pub mod cli;
pub mod process;
pub mod report;
pub mod resource;
pub mod target;
pub mod workspace;

pub async fn run_from<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match cli::parse_config_from(args) {
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
