use std::fmt::Write as _;
use std::path::Path;

use clap::{Arg, ArgAction, ColorChoice, Command};

const REGENERATE: &str = "cargo run --locked -p hoimin-cli --example generate_cli_reference";

fn visible(arg: &Arg) -> bool {
    !arg.is_hide_set() && !arg.is_hide_long_help_set()
}

fn spelling(arg: &Arg) -> String {
    arg.get_long().map_or_else(
        || {
            arg.get_short()
                .map_or_else(|| arg.get_id().to_string(), |short| format!("-{short}"))
        },
        |long| format!("--{long}"),
    )
}

fn code_block(output: &mut String, text: &str) {
    // A longer fence also handles literal backticks in future help strings.
    let fence = "`".repeat(
        text.split(|c| c != '`')
            .map(str::len)
            .max()
            .unwrap_or(0)
            .max(2)
            + 1,
    );
    writeln!(output, "{fence}text\n{}\n{fence}\n", text.trim_end()).unwrap();
}

fn render(mut command: Command) -> String {
    command.build();
    let mut output =
        String::from("# CLI reference\n\n<!-- Generated; do not edit by hand. -->\n\n");
    writeln!(output, "Source: [Rust CLI definitions](../crates/hoimin-cli/src/cli.rs).\n\nRegenerate with `{REGENERATE}`; append ` -- --check` to verify without writing.\n").unwrap();
    output.push_str("See the [usage guide](usage.md) for examples, OS-specific resource limits,\nplan/verify inheritance and validation beyond Clap. Conditional requirements\nand numeric parser bounds have no general stable Clap reflection API; their\nhelp text and the manual guide remain authoritative. The metadata below lists\nreflected arity, repetition, required arguments/groups, conflicts and delimiters.\n\n");
    let name = command.get_name().to_owned();
    render_command(&mut command, &name, &mut output);
    let normalized = output
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}\n", normalized.trim_end())
}

fn render_command(command: &mut Command, path: &str, output: &mut String) {
    writeln!(output, "## {path}\n").unwrap();
    let aliases = command.get_visible_aliases().collect::<Vec<_>>();
    if !aliases.is_empty() {
        code_block(output, &format!("Command aliases: {}", aliases.join(", ")));
    }
    // Override terminal-dependent formatting on every command, including children.
    let mut help_command = command
        .clone()
        .color(ColorChoice::Never)
        .term_width(100)
        .max_term_width(100);
    code_block(output, &help_command.render_long_help().to_string());
    let mut metadata = String::new();
    for arg in command.get_arguments().filter(|arg| visible(arg)) {
        render_argument(command, arg, &mut metadata);
    }
    render_groups(command, &mut metadata);
    if !metadata.is_empty() {
        output.push_str("Argument constraints:\n\n");
        code_block(output, &metadata);
    }
    for child in command
        .get_subcommands_mut()
        .filter(|child| !child.is_hide_set())
    {
        let child_path = format!("{path} {}", child.get_name());
        render_command(child, &child_path, output);
    }
}

fn render_argument(command: &Command, arg: &Arg, metadata: &mut String) {
    writeln!(metadata, "{}", spelling(arg)).unwrap();
    writeln!(metadata, "  required: {}", arg.is_required_set()).unwrap();
    if let Some(arity) = arg.get_num_args() {
        writeln!(metadata, "  arity: {arity}").unwrap();
    }
    writeln!(
        metadata,
        "  repeatable: {}",
        matches!(arg.get_action(), ArgAction::Append | ArgAction::Count)
    )
    .unwrap();
    if !arg.is_hide_default_value_set() && !arg.get_default_values().is_empty() {
        let defaults = arg
            .get_default_values()
            .iter()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>();
        writeln!(metadata, "  default: {}", defaults.join(", ")).unwrap();
    }
    if !arg.is_hide_possible_values_set() {
        let values = arg
            .get_possible_values()
            .into_iter()
            .filter(|value| !value.is_hide_set())
            .map(|value| value.get_name().to_owned())
            .collect::<Vec<_>>();
        if !values.is_empty() {
            writeln!(metadata, "  values: {}", values.join(", ")).unwrap();
        }
    }
    let conflicts = command
        .get_arg_conflicts_with(arg)
        .into_iter()
        .filter(|arg| visible(arg))
        .map(spelling)
        .collect::<Vec<_>>();
    if !conflicts.is_empty() {
        writeln!(metadata, "  conflicts: {}", conflicts.join(", ")).unwrap();
    }
    if let Some(delimiter) = arg.get_value_delimiter() {
        writeln!(metadata, "  delimiter: {delimiter}").unwrap();
    }
    let aliases = arg
        .get_visible_aliases()
        .unwrap_or_default()
        .into_iter()
        .map(|alias| format!("--{alias}"))
        .chain(
            arg.get_visible_short_aliases()
                .unwrap_or_default()
                .into_iter()
                .map(|alias| format!("-{alias}")),
        )
        .collect::<Vec<_>>();
    if !aliases.is_empty() {
        writeln!(metadata, "  aliases: {}", aliases.join(", ")).unwrap();
    }
    if arg.is_last_set() {
        writeln!(metadata, "  follows: --").unwrap();
    }
}

fn render_groups(command: &Command, metadata: &mut String) {
    for group in command.get_groups().filter(|group| group.is_required_set()) {
        let members = group
            .get_args()
            .filter_map(|id| command.get_arguments().find(|arg| arg.get_id() == id))
            .filter(|arg| visible(arg))
            .map(spelling)
            .collect::<Vec<_>>();
        if !members.is_empty() {
            let mut group = group.clone();
            let multiple = group.is_multiple();
            writeln!(
                metadata,
                "required group {}: {}; multiple: {multiple}",
                group.get_id(),
                members.join(", ")
            )
            .unwrap();
        }
    }
}

fn update(path: &Path, text: &str, check: bool) -> Result<(), String> {
    if check {
        let existing = std::fs::read_to_string(path)
            .map_err(|error| format!("Cannot read CLI reference: {error}. Run `{REGENERATE}`."))?;
        if existing.replace("\r\n", "\n") != text {
            return Err(format!("CLI reference is stale. Run `{REGENERATE}`."));
        }
        Ok(())
    } else {
        std::fs::write(path, text).map_err(|error| format!("Cannot write CLI reference: {error}"))
    }
}

fn main() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let check = match args.as_slice() {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => return Err(format!("Usage: {REGENERATE} [-- --check]")),
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli-reference.md");
    update(&path, &render(hoimin_cli::cli::root_command()), check)
}

#[cfg(test)]
mod tests {
    use super::render;
    use clap::{Arg, ArgAction, ArgGroup, Command, builder::PossibleValue};

    #[test]
    fn actual_reference_is_current_and_covers_public_commands() {
        let text = render(hoimin_cli::cli::root_command());
        for command in ["run", "plan", "verify", "progress", "completions"] {
            assert!(text.contains(&format!("## hoimin {command}\n")));
        }
        for expected in [
            "--max-workspace-size",
            "default: 8GiB",
            "--min-free-space",
            "default: 10GiB",
            "values: strict, diverse, line-diverse",
            "values: bash, elvish, fish, powershell, zsh",
            "required group selection: --candidate, --top, --sample; multiple: false",
            "Execution and resource settings come from PLAN",
            "follows: --",
        ] {
            assert!(text.contains(expected), "missing {expected}");
        }
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/cli-reference.md");
        super::update(&path, &text, true).unwrap();
        assert!(!text.contains(env!("CARGO_MANIFEST_DIR")));
    }

    #[test]
    fn changed_help_defaults_and_new_options_make_reference_stale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reference.md");
        let original = Command::new("fixture").arg(
            Arg::new("mode")
                .long("mode")
                .help("Old help")
                .default_value("old"),
        );
        let original_text = render(original.clone());
        let changed = [
            original.clone().mut_arg("mode", |arg| arg.help("New help")),
            original
                .clone()
                .mut_arg("mode", |arg| arg.default_value("new")),
            original.arg(Arg::new("added").long("added")),
        ];
        for command in changed {
            super::update(&path, &original_text, false).unwrap();
            let text = render(command);
            assert!(
                super::update(&path, &text, true)
                    .unwrap_err()
                    .contains("stale")
            );
            super::update(&path, &text, false).unwrap();
            super::update(&path, &text, true).unwrap();
        }
    }

    #[test]
    fn help_with_markdown_fences_stays_inside_code_block() {
        let mut text = String::new();
        super::code_block(&mut text, "```\n# literal heading\n```");
        assert!(text.starts_with("````text\n"));
        assert!(text.ends_with("\n````\n\n"));
    }

    #[test]
    fn check_detects_staleness_without_writing_and_accepts_checkout_newlines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reference.md");
        assert!(super::update(&path, "expected\n", true).is_err());
        assert!(!path.exists());
        super::update(&path, "expected\n", false).unwrap();
        super::update(&path, "expected\n", true).unwrap();
        std::fs::write(&path, "expected\r\n").unwrap();
        super::update(&path, "expected\n", true).unwrap();
        assert!(super::update(&path, "changed\n", true).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"expected\r\n");
    }

    #[test]
    fn reflects_help_defaults_values_and_constraints() {
        let command = Command::new("fixture").subcommand(
            Command::new("select")
                .about("Choose a value")
                .arg(
                    Arg::new("mode")
                        .long("mode")
                        .help("Selection mode")
                        .value_parser([
                            PossibleValue::new("fast"),
                            PossibleValue::new("secret").hide(true),
                        ])
                        .default_value("fast")
                        .conflicts_with("all"),
                )
                .arg(Arg::new("all").long("all").action(ArgAction::SetTrue))
                .arg(Arg::new("input").required(true).num_args(2..))
                .group(
                    ArgGroup::new("selection")
                        .args(["mode", "all"])
                        .required(true),
                ),
        );
        let text = render(command);
        for expected in [
            "## fixture select",
            "Choose a value",
            "Selection mode",
            "fast",
            "default: fast",
            "values: fast",
            "conflicts: --all",
            "required: true",
            "arity: 2..",
            "required group selection",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        assert!(!text.contains("secret"));
    }

    #[test]
    fn excludes_hidden_items_and_keeps_nested_commands() {
        let command = Command::new("fixture")
            .arg(Arg::new("hidden").long("hidden").hide(true))
            .arg(
                Arg::new("public")
                    .long("public")
                    .alias("private-alias")
                    .visible_alias("visible-alias"),
            )
            .subcommand(Command::new("internal").hide(true))
            .subcommand(
                Command::new("outer").subcommand(Command::new("inner").about("Nested help")),
            );
        let text = render(command);
        assert!(text.contains("## fixture outer inner"));
        assert!(text.contains("visible-alias"));
        for hidden in ["--hidden", "private-alias", "fixture internal"] {
            assert!(!text.contains(hidden), "leaked {hidden}: {text}");
        }
    }

    #[test]
    fn rendering_is_repeatable_and_plain_text() {
        let command = Command::new("fixture").about("A stable command").arg(
            Arg::new("value")
                .long("value")
                .help("A long description ".repeat(30)),
        );
        let first = render(command.clone());
        assert!(first.contains("A stable command"));
        assert_eq!(first, render(command));
        assert!(!first.contains('\r'));
        assert!(!first.contains('\u{1b}'));
        assert!(first.lines().all(|line| line == line.trim_end()));
        assert!(first.ends_with('\n'));
        assert!(!first.ends_with("\n\n"));
    }
}
