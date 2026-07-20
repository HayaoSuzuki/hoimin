use std::ffi::OsString;

pub mod analyzer;
pub mod cli;
pub mod process;
pub mod progress;
pub mod report;
pub mod resource;
pub mod session;
pub mod shell;
pub mod target;
pub mod workspace;

pub async fn run_from<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    run_with_io(args, &mut stdout, &mut stderr).await
}

pub async fn run_with_io<I, T, Stdout, Stderr>(
    args: I,
    stdout: &mut Stdout,
    stderr: &mut Stderr,
) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    Stdout: std::io::Write,
    Stderr: std::io::Write,
{
    match cli::parse_config_from(args) {
        Ok(config) => match shell::run_loop(config, &mut *stdout, &mut *stderr).await {
            Ok(code) => code,
            Err(error) => {
                let _ = writeln!(stderr, "{error}");
                2
            }
        },
        Err(cli::CliError::Clap(error)) => {
            let exit_code = error.exit_code();
            let _ = writeln!(stderr, "{error}");
            exit_code
        }
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            2
        }
    }
}
