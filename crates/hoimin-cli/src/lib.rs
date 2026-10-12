use std::ffi::OsString;

pub mod analyzer;
mod artifacts;
mod baseline_output;
pub mod cli;
mod copy_policy;
mod fingerprint_env;
pub mod fingerprint_inputs;
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

pub async fn run_from<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let result = match cli::parse_from(args) {
        Ok(cli::ParsedCommand::Run(args)) => match cli::run_config_from_args(args) {
            Ok(config) => {
                shell::run_owned_loop(
                    config,
                    std::io::stdout(),
                    std::io::stderr(),
                    shell::RunControl::new(),
                )
                .await
            }
            Err(error) => Err(error.to_string()),
        },
        Ok(cli::ParsedCommand::Verify(args)) => {
            match plan::prepare_verify_selection(&args.manifest, &args.selection, args.format).await
            {
                Ok(verified) if args.dry_run => verified
                    .preview()
                    .write(args.format, &mut std::io::stdout())
                    .map(|()| 0),
                Ok(verified) => {
                    let verified = verified.with_metrics(args.metrics);
                    shell::run_owned_verified(verified, std::io::stdout(), std::io::stderr()).await
                }
                Err(error) => Err(error.to_string()),
            }
        }
        command => {
            return run_parsed_with_io(command, &mut std::io::stdout(), &mut std::io::stderr())
                .await;
        }
    };
    match result {
        Ok(code) => code,
        Err(error) => {
            let diagnostic = tokio::task::spawn_blocking(move || {
                use std::io::Write;
                let _ = writeln!(std::io::stderr(), "{error}");
            });
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), diagnostic).await;
            2
        }
    }
}

/// Runs commands with caller-provided synchronous writers, including borrowed writers.
/// Blocking writes on this compatibility API are not interruptible; `run_from`
/// uses owned, deadline-aware report delivery for CLI run and verify commands.
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
    Box::pin(run_parsed_with_io(cli::parse_from(args), stdout, stderr)).await
}

async fn run_parsed_with_io<Stdout: std::io::Write, Stderr: std::io::Write>(
    command: Result<cli::ParsedCommand, cli::CliError>,
    stdout: &mut Stdout,
    stderr: &mut Stderr,
) -> i32 {
    match command {
        Ok(cli::ParsedCommand::Run(args)) => {
            let result = match cli::run_config_from_args(args) {
                Ok(config) => shell::run_loop(config, &mut *stdout, &mut *stderr).await,
                Err(error) => Err(error.to_string()),
            };
            command_result(result, stderr)
        }
        Ok(cli::ParsedCommand::Progress(args)) => {
            command_result(progress::run(args, stdout, stderr), stderr)
        }
        Ok(cli::ParsedCommand::Review(args)) => command_result(review::run(&args, stdout), stderr),
        Ok(cli::ParsedCommand::Completions(args)) => {
            cli::write_completions(args.shell, stdout);
            0
        }
        Ok(cli::ParsedCommand::Plan(args)) => {
            let result = async {
                let config = args.into_run_config().map_err(|error| error.to_string())?;
                let output = plan::create(config)
                    .await
                    .map_err(|error| error.to_string())?;
                serde_json::to_writer(&mut *stdout, &output.manifest)
                    .map_err(|error| error.to_string())?;
                writeln!(stdout).map_err(|error| error.to_string())?;
                Ok::<_, String>(output.exit_code)
            }
            .await;
            command_result(result, stderr)
        }
        Ok(cli::ParsedCommand::Verify(args)) => {
            let result =
                match plan::prepare_verify_selection(&args.manifest, &args.selection, args.format)
                    .await
                {
                    Ok(verified) if args.dry_run => {
                        verified.preview().write(args.format, stdout).map(|()| 0)
                    }
                    Ok(verified) => {
                        let verified = verified.with_metrics(args.metrics);
                        verified.execute(stdout, stderr).await
                    }
                    Err(error) => Err(error.to_string()),
                };
            command_result(result, stderr)
        }
        Err(cli::CliError::Clap(error)) => {
            let exit_code = error.exit_code();
            if exit_code == 0 {
                let _ = write!(stdout, "{error}");
            } else {
                let _ = write!(stderr, "{error}");
            }
            exit_code
        }
        Err(error) => command_result(Err(error), stderr),
    }
}

fn command_result(
    result: Result<i32, impl std::fmt::Display>,
    stderr: &mut impl std::io::Write,
) -> i32 {
    match result {
        Ok(code) => code,
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            2
        }
    }
}
