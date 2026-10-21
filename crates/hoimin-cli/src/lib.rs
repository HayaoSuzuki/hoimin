use std::ffi::OsString;

pub mod analyzer;
mod artifacts;
mod baseline_output;
pub mod cli;
mod copy_policy;
mod fingerprint_env;
pub mod fingerprint_inputs;
mod finite_file;
mod human_text;
mod interrupt;
mod live_progress;
mod metrics;
mod metrics_destination;
pub mod plan;
mod portable_path;
pub mod process;
pub mod progress;
pub mod report;
pub mod resource;
pub mod review;
pub mod session;
pub mod shell;
pub mod target;
pub mod workspace;

#[cfg(test)]
#[path = "../tests/support/collecting_writer.rs"]
mod test_writer;

pub async fn run_from<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    run_with_writers(args, std::io::stdout(), std::io::stderr()).await
}

/// Runs CLI commands with owned output writers.
/// Run and verify delivery shares the CLI shutdown deadline. A started blocking
/// write retains its writer and delivery resources until the operation returns.
/// Short commands use the locally owned writers synchronously.
///
/// Borrowed outputs cannot enter the run lifecycle:
/// ```compile_fail
/// async fn borrowed() {
///     let mut output = Vec::new();
///     hoimin_cli::run_with_writers(["hoimin", "--help"], &mut output, std::io::sink()).await;
/// }
/// ```
pub async fn run_with_writers<I, T, Stdout, Stderr>(args: I, stdout: Stdout, stderr: Stderr) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    Stdout: std::io::Write + Send + 'static,
    Stderr: std::io::Write + Send + 'static,
{
    Box::pin(run_command(cli::parse_from(args), stdout, stderr)).await
}

async fn run_command<Stdout, Stderr>(
    command: Result<cli::ParsedCommand, cli::CliError>,
    mut stdout: Stdout,
    mut stderr: Stderr,
) -> i32
where
    Stdout: std::io::Write + Send + 'static,
    Stderr: std::io::Write + Send + 'static,
{
    match command {
        Ok(cli::ParsedCommand::Run(args)) => {
            let presentation = report::ReportPresentation {
                verbose: args.verbose,
            };
            let config = match cli::run_config_from_args(args) {
                Ok(config) => config,
                Err(error) => {
                    write_terminal_error(error.to_string(), stderr).await;
                    return 2;
                }
            };
            shell::run_loop(config, stdout, stderr, presentation)
                .await
                .unwrap_or(2)
        }
        Ok(cli::ParsedCommand::Progress(args)) => {
            command_result(progress::run(args, &mut stdout, &mut stderr), &mut stderr)
        }
        Ok(cli::ParsedCommand::Review(args)) => {
            command_result(review::run(&args, &mut stdout), &mut stderr)
        }
        Ok(cli::ParsedCommand::Completions(args)) => {
            cli::write_completions(args.shell, &mut stdout);
            0
        }
        Ok(cli::ParsedCommand::Plan(args)) => {
            let result = async {
                let (config, discovery) = args.into_configs().map_err(|error| error.to_string())?;
                let output = plan::create(config, discovery)
                    .await
                    .map_err(|error| error.to_string())?;
                serde_json::to_writer(&mut stdout, &output.manifest)
                    .map_err(|error| error.to_string())?;
                writeln!(stdout).map_err(|error| error.to_string())?;
                Ok::<_, String>(output.exit_code)
            }
            .await;
            command_result(result, &mut stderr)
        }
        Ok(cli::ParsedCommand::Verify(args)) => {
            if args.dry_run {
                match plan::prepare_verify_preview_selection(
                    &args.manifest,
                    &args.selection,
                    args.format,
                )
                .await
                {
                    Ok(preview) => command_result(
                        preview.write(args.format, &mut stdout).map(|()| 0),
                        &mut stderr,
                    ),
                    Err(error) => {
                        write_terminal_error(error.to_string(), stderr).await;
                        2
                    }
                }
            } else {
                let verified = match plan::prepare_verify_selection(
                    &args.manifest,
                    &args.selection,
                    args.format,
                )
                .await
                {
                    Ok(verified) => verified,
                    Err(error) => {
                        write_terminal_error(error.to_string(), stderr).await;
                        return 2;
                    }
                };
                verified
                    .with_metrics(args.metrics)
                    .execute(
                        stdout,
                        stderr,
                        report::ReportPresentation {
                            verbose: args.verbose,
                        },
                    )
                    .await
                    .unwrap_or(2)
            }
        }
        Err(cli::CliError::Clap(mut error)) => {
            human_text::escape_clap_context(&mut error);
            let exit_code = error.exit_code();
            if exit_code == 0 {
                let _ = write!(stdout, "{error}");
            } else {
                let _ = write!(stderr, "{error}");
            }
            exit_code
        }
        Err(error) => command_result(Err(error), &mut stderr),
    }
}

/// Preparation failures occur before report delivery owns stderr. Bound this
/// final diagnostic wait too; an in-flight write keeps ownership after expiry.
pub(crate) async fn write_terminal_error<Stderr>(error: String, mut stderr: Stderr)
where
    Stderr: std::io::Write + Send + 'static,
{
    let diagnostic = tokio::task::spawn_blocking(move || {
        let _ = writeln!(stderr, "{}", human_text::HumanText(&error));
    });
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), diagnostic).await;
}

fn command_result(
    result: Result<i32, impl std::fmt::Display>,
    stderr: &mut impl std::io::Write,
) -> i32 {
    match result {
        Ok(code) => code,
        Err(error) => {
            let _ = writeln!(stderr, "{}", human_text::HumanText(&error.to_string()));
            2
        }
    }
}
