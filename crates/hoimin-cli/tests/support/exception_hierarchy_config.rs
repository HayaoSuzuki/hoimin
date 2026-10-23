//! CLI configuration for the shared exception-hierarchy oracle corpus.

use std::path::Path;

use super::corpus::Case;

pub fn config<E: std::fmt::Debug>(
    case: &Case,
    root: &Path,
    parse_config: impl FnOnce(
        Vec<String>,
    ) -> Result<hoimin_core::RunConfig<hoimin_core::EnvironmentRequest>, E>,
) -> hoimin_core::RunConfig<hoimin_core::EnvironmentRequest> {
    let limit = case.max_candidates.to_string();
    let mut args = vec![
        "hoimin",
        "run",
        "--root",
        root.to_str().unwrap(),
        "--file",
        "service.py",
        "--operators",
        "exception_hierarchy",
        "--allow-best-effort-memory",
        "--min-free-space",
        "1B",
        "--max-candidates",
        &limit,
    ];
    for import_root in &case.roots {
        args.extend(["--import-root", import_root]);
    }
    args.extend(["--", "unused-test-command"]);
    // Both adapters use the real CLI configuration parser.
    parse_config(args.into_iter().map(str::to_owned).collect()).unwrap()
}
