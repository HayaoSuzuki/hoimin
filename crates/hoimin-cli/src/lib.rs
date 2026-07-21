use std::ffi::OsString;

pub mod analyzer;
pub mod cli;
pub mod fingerprint_inputs;
pub mod plan;
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
    match cli::parse_from(args) {
        Ok(cli::ParsedCommand::Run(args)) => match cli::run_config_from_args(args) {
            Ok(config) => match shell::run_loop(config, &mut *stdout, &mut *stderr).await {
                Ok(code) => code,
                Err(error) => {
                    let _ = writeln!(stderr, "{error}");
                    2
                }
            },
            Err(error) => {
                let _ = writeln!(stderr, "{error}");
                2
            }
        },
        Ok(cli::ParsedCommand::Progress(args)) => match progress::run(args, stdout, stderr) {
            Ok(code) => code,
            Err(error) => {
                let _ = writeln!(stderr, "{error}");
                2
            }
        },
        Ok(cli::ParsedCommand::Plan(args)) => match args.into_run_config() {
            Ok(config) => match plan::create(config).await {
                Ok(output) => match serde_json::to_writer(&mut *stdout, &output.manifest) {
                    Ok(()) => match writeln!(stdout) {
                        Ok(()) => output.exit_code,
                        Err(error) => {
                            let _ = writeln!(stderr, "{error}");
                            2
                        }
                    },
                    Err(error) => {
                        let _ = writeln!(stderr, "{error}");
                        2
                    }
                },
                Err(error) => {
                    let _ = writeln!(stderr, "{error}");
                    2
                }
            },
            Err(error) => {
                let _ = writeln!(stderr, "{error}");
                2
            }
        },
        Ok(cli::ParsedCommand::Verify(_)) => {
            let _ = writeln!(
                stderr,
                "the `verify` command is not available in this build"
            );
            2
        }
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
