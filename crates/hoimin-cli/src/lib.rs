use std::ffi::OsString;

pub mod analyzer;
pub mod cli;
pub mod fingerprint_inputs;
mod interrupt;
mod metrics;
pub mod plan;
mod portable_path;
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
                Ok(verified) => {
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

#[expect(
    clippy::too_many_lines,
    reason = "top-level command dispatch keeps each CLI result and diagnostic path explicit"
)]
async fn run_parsed_with_io<Stdout: std::io::Write, Stderr: std::io::Write>(
    command: Result<cli::ParsedCommand, cli::CliError>,
    stdout: &mut Stdout,
    stderr: &mut Stderr,
) -> i32 {
    match command {
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
        Ok(cli::ParsedCommand::Completions(args)) => {
            cli::write_completions(args.shell, stdout);
            0
        }
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
        Ok(cli::ParsedCommand::Verify(args)) => {
            match plan::prepare_verify_selection(&args.manifest, &args.selection, args.format).await
            {
                Ok(verified) => {
                    let verification_selection = verified.verification_selection;
                    let result = match verified.selection {
                        plan::ResolvedVerifySelection::ExplicitCandidates(candidate_ids) => {
                            shell::run_selected_loop_with_fingerprint_inputs(
                                verified.config,
                                candidate_ids,
                                verification_selection,
                                verified.fingerprint_copy_inputs,
                                &mut *stdout,
                                &mut *stderr,
                            )
                            .await
                        }
                        plan::ResolvedVerifySelection::RankedCandidates(candidate_ids) => {
                            shell::run_ordered_selected_loop_with_fingerprint_inputs(
                                verified.config,
                                candidate_ids,
                                verification_selection,
                                verified.fingerprint_copy_inputs,
                                &mut *stdout,
                                &mut *stderr,
                            )
                            .await
                        }
                    };
                    match result {
                        Ok(code) => code,
                        Err(error) => {
                            let _ = writeln!(stderr, "{error}");
                            2
                        }
                    }
                }
                Err(error) => {
                    let _ = writeln!(stderr, "{error}");
                    2
                }
            }
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
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            2
        }
    }
}
