use std::fmt::Write as _;
use std::path::Path;

use clap::{Arg, ArgAction, Command};

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
    let mut output = String::from("<!-- Generated; do not edit by hand. -->\n\n");
    write!(
        output,
        include_str!("../docs/cli-reference.md"),
        regenerate = REGENERATE
    )
    .unwrap();
    output.push('\n');
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
    // Fix help width on every command, including children.
    let mut help_command = command.clone().term_width(100).max_term_width(100);
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
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/user/reference/cli.md");
    update(&path, &render_reference(), check)
}

fn render_reference() -> String {
    let command = hoimin_cli::cli::root_command();
    let mut reference = render(command.clone());
    render_operator_index(&command, &mut reference);
    reference
}

fn render_operator_index(command: &Command, output: &mut String) {
    use hoimin_core::{MutationOperator, MutationOperatorSelection};

    let defaults = MutationOperatorSelection::default();
    let names = MutationOperatorSelection::valid_names();
    let families: Vec<_> = names
        .iter()
        .copied()
        .filter(|name| MutationOperator::from_name(name).is_none())
        .map(|name| {
            (
                name,
                MutationOperatorSelection::parse_selector(name).unwrap(),
            )
        })
        .collect();
    writeln!(
        output,
        "\n{}",
        include_str!("../docs/operator-selection.md")
    )
    .unwrap();
    let run = command.find_subcommand("run").expect("public run command");
    for id in ["operators", "exclude_operators"] {
        let arg = run
            .get_arguments()
            .find(|arg| arg.get_id() == id)
            .expect("selection argument");
        let description = arg
            .get_long_help()
            .or_else(|| arg.get_help())
            .expect("selection help");
        writeln!(output, "### `{}`\n\n{description}\n", spelling(arg)).unwrap();
    }
    output.push_str("### Operator IDs\n\n| Operator ID | Default runtime selection | Selector families |\n| --- | --- | --- |\n");
    for name in names {
        let Some(operator) = MutationOperator::from_name(name) else {
            continue;
        };
        let selected = if defaults.contains(operator) {
            "yes"
        } else {
            "opt-in"
        };
        let members: Vec<_> = families
            .iter()
            .filter(|(_, members)| members.contains(&operator))
            .map(|(family, _)| format!("[`{family}`](#{family})"))
            .collect();
        let family = if members.is_empty() {
            "—".to_owned()
        } else {
            members.join(", ")
        };
        writeln!(
            output,
            "| [`{name}`](operators.md#{name}) | {selected} | {family} |"
        )
        .unwrap();
    }
    writeln!(
        output,
        "\n{}",
        include_str!("../docs/selector-families.md").trim_end()
    )
    .unwrap();
    for (family, members) in families {
        writeln!(output, "\n### `{family}`\n").unwrap();
        for member in members {
            let name = member.as_str();
            writeln!(output, "- [`{name}`](operators.md#{name})").unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::render;
    use clap::{Arg, ArgAction, ArgGroup, Command, builder::PossibleValue};

    proptest::proptest! {
        #[test]
        fn long_help_words_reach_the_website_without_rewriting(
            words in proptest::collection::vec("[a-zA-Zα-ωぁ-ん]{1,12}", 1..30),
            windows_newlines in proptest::bool::ANY,
        ) {
            // Arrange
            let separator = if windows_newlines { "\r\n" } else { "\n" };
            let explanation = words.join(separator);
            let command = Command::new("fixture").arg(
                Arg::new("selected").long("selected").long_help(explanation.clone())
            );

            // Act
            let reference = render(command);

            // Assert
            proptest::prop_assert!(reference.split_whitespace().collect::<Vec<_>>().join(" ")
                .contains(&explanation.split_whitespace().collect::<Vec<_>>().join(" ")));
        }
    }

    #[test]
    fn actual_reference_is_current_and_covers_public_commands() {
        let text = super::render_reference();
        for command in ["run", "plan", "verify", "progress", "review"] {
            assert!(text.contains(&format!("## hoimin {command}\n")));
        }
        for expected in [
            "--max-workspace-size",
            "default: 8GiB",
            "--min-free-space",
            "default: 10GiB",
            "values: strict, diverse, line-diverse",
            "required group selection: --candidate, --top, --sample; multiple: false",
            "Execution and resource settings come from PLAN",
            "follows: --",
        ] {
            assert!(text.contains(expected), "missing {expected}");
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/user/reference/cli.md");
        super::update(&path, &text, true).unwrap();
        assert!(!text.contains(env!("CARGO_MANIFEST_DIR")));
    }

    #[test]
    fn operator_index_links_each_id_with_its_actual_default_status() {
        // Arrange
        let defaults = hoimin_core::MutationOperatorSelection::default();
        let names = hoimin_core::MutationOperatorSelection::valid_names();

        // Act
        let reference = super::render_reference();

        // Assert: registry changes must reach the generated, linked selection index.
        for name in names {
            if let Some(operator) = hoimin_core::MutationOperator::from_name(name) {
                let selected = if defaults.contains(operator) {
                    "yes"
                } else {
                    "opt-in"
                };
                let row = format!("| [`{name}`](operators.md#{name}) | {selected} |");
                assert!(
                    reference.contains(&row),
                    "missing selection metadata for {name}"
                );
            }
        }
    }

    #[test]
    fn operator_index_expands_every_advertised_family() {
        // Arrange
        let families: Vec<_> = hoimin_core::MutationOperatorSelection::valid_names()
            .into_iter()
            .filter(|name| hoimin_core::MutationOperator::from_name(name).is_none())
            .collect();

        // Act
        let reference = super::render_reference();

        // Assert
        for family in families {
            let heading = format!("### `{family}`\n");
            let section = reference
                .split_once(&heading)
                .unwrap()
                .1
                .split("\n### ")
                .next()
                .unwrap();
            let actual: Vec<_> = section
                .lines()
                .filter_map(|line| {
                    line.strip_prefix("- [`")
                        .and_then(|link| link.split_once("`](").map(|(id, _)| id))
                })
                .collect();
            let expected: Vec<_> = hoimin_core::MutationOperatorSelection::parse_selector(family)
                .unwrap()
                .into_iter()
                .map(hoimin_core::MutationOperator::as_str)
                .collect();
            assert_eq!(actual, expected, "incorrect family expansion for {family}");
        }
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
