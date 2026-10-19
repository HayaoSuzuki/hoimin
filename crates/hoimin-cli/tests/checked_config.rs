use hoimin_core::{CommandArg, EnvironmentRequest, RawRunConfig, RunConfig};
use serde_json::{Value, json};

fn preparation_input(root: &std::path::Path) -> Value {
    let config = RunConfig::try_from(RawRunConfig {
        root: camino::Utf8PathBuf::from_path_buf(root.to_owned()).unwrap(),
        files: vec!["calc.py".into()],
        fingerprint_files: vec!["missing-fingerprint.txt".into()],
        test_argv: vec![CommandArg::Unix(b"python".to_vec())],
        ..RawRunConfig::default()
    })
    .unwrap();
    serde_json::to_value(config).unwrap()
}

fn reject_before_preparation(change: fn(&mut Value), expected: &str) {
    // Arrange: resolving fingerprint inputs would fail on this missing file.
    let project = tempfile::tempdir().unwrap();
    let mut input = preparation_input(project.path());
    change(&mut input);
    let mut preparation_entered = false;

    // Act: the public accepted-config boundary precedes public preparation.
    let result = serde_json::from_value::<RunConfig<EnvironmentRequest>>(input)
        .map_err(|error| error.to_string())
        .and_then(|config| {
            preparation_entered = true;
            hoimin_cli::shell::prepare_run_config(config).map_err(|error| error.to_string())
        });

    // Assert: a filesystem failure must not mask semantic acceptance.
    let error = result.unwrap_err();
    assert!(error.contains(expected), "{error}");
    assert!(!preparation_entered);
}

macro_rules! invalid_preparation_cases {
    ($($name:ident: $change:expr => $expected:literal;)*) => {
        $(#[test]
        fn $name() {
            reject_before_preparation($change, $expected);
        })*
    };
}

invalid_preparation_cases! {
    rejects_jobs_above_absolute_cap: |input| {
        input["limits"]["jobs"] = json!(257);
        input["limits"]["max_processes"] = json!(300);
    } => "--jobs 257 exceeds the supported maximum 256";
    rejects_jobs_above_process_cap: |input| input["limits"]["jobs"] = json!(65)
        => "--jobs 65 exceeds --max-processes 64";
    rejects_empty_required_selector: |input| input["selection"]["files"] = json!([])
        => "at least one target selector is required";
    rejects_empty_operators: |input| input["operators"] = json!([])
        => "mutation operator selection is empty";
    rejects_empty_test_argv: |input| input["test_argv"] = json!([])
        => "at least one test argv element is required";
    rejects_escaping_import_root: |input| input["import_roots"] = json!(["../escape"])
        => "invalid --import-root ../escape";
}

#[test]
fn valid_configuration_reaches_fingerprint_resolution() {
    // Arrange
    let project = tempfile::tempdir().unwrap();
    let config =
        serde_json::from_value::<RunConfig<EnvironmentRequest>>(preparation_input(project.path()))
            .unwrap();

    // Act
    let error = hoimin_cli::shell::prepare_run_config(config).unwrap_err();

    // Assert
    assert!(error.to_string().contains("fingerprint.file.not_found"));
}
