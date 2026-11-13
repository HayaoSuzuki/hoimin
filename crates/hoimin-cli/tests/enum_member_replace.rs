#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/jsonl_report.rs"]
mod jsonl_report;
use collecting_writer::CollectingWriter;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn python() -> PathBuf {
    std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap();
            root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        },
        PathBuf::from,
    )
}

async fn compile(source: &str) -> std::process::Output {
    tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(python())
            .args([
                "-c",
                "import sys; assert sys.version_info[:2] in ((3, 14), (3, 15)), sys.version; compile(sys.argv[1], '<neighbor>', 'exec')",
                source,
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("CPython compiler deadline")
    .expect("CPython 3.14 or 3.15; set HOIMIN_OPERATOR_TEST_PYTHON to its executable")
}

async fn cli(args: Vec<OsString>) -> (i32, serde_json::Value) {
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();
    let code = hoimin_cli::run_with_writers(args, stdout.clone(), stderr.clone()).await;
    assert!(
        code == 0,
        "exit={code}, stderr={}",
        String::from_utf8_lossy(&stderr.snapshot())
    );
    (
        code,
        jsonl_report::from_slice(&stdout.snapshot()).expect("one JSON document"),
    )
}

async fn plan(root: &Path, operators: &str, test: &str) -> serde_json::Value {
    let args = [
        OsString::from("hoimin"),
        "plan".into(),
        "--root".into(),
        root.as_os_str().to_owned(),
        "--file".into(),
        "subject.py".into(),
        "--operators".into(),
        operators.into(),
        "--min-free-space".into(),
        "1B".into(),
        "--baseline-timeout".into(),
        "10s".into(),
        "--mutant-timeout".into(),
        "10s".into(),
        "--total-timeout".into(),
        "30s".into(),
        "--allow-best-effort-memory".into(),
        "--".into(),
        python().into_os_string(),
        "-B".into(),
        "-c".into(),
        test.into(),
    ];
    let (code, document) = cli(args.into()).await;
    assert_eq!(code, 0);
    document
}

#[rstest::rstest]
#[case::integer_members("Enum", "A=1; B=2; C=3", vec!["B", "C"])]
#[case::integer_alias("IntEnum", "A=-1; Alias=-1; B=2", vec!["B"])]
#[case::string_alias("Enum", "A='a'; Alias='a'; B='b'", vec!["B"])]
#[case::automatic_values("Enum", "A=auto(); B=auto(); C=auto()", vec!["B", "C"])]
#[case::case_folded_string_alias("StrEnum", "A=auto(); a=auto(); B=auto()", vec!["B"])]
#[case::aliases_without_destination("Enum", "A=1; Alias=1", vec![])]
#[tokio::test]
async fn enum_members_are_distinct_canonical_destinations(
    #[case] base: &str,
    #[case] members: &str,
    #[case] expected: Vec<&str>,
) {
    let root = tempfile::tempdir().unwrap();
    let source = format!(
        "from enum import {base} as Base, auto\nclass Status(Base):\n    {members}\ndef choose():\n    return Status.A\n"
    );
    std::fs::write(root.path().join("subject.py"), &source).unwrap();

    let doc = plan(root.path(), "enum_member_replace", "pass").await;
    let cs = doc["candidates"].as_array().unwrap();
    let mut actual: Vec<_> = cs
        .iter()
        .map(|c| c["replacement"].as_str().unwrap())
        .collect();
    actual.sort_unstable();

    let replay = plan(root.path(), "enum_member_replace", "pass").await;
    let mut compiled = Vec::new();
    for c in cs {
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let mut mutant = source.clone();
        mutant.replace_range(start..=start, c["replacement"].as_str().unwrap());
        compiled.push(compile(&mutant).await);
    }

    assert_eq!(actual, expected, "{source} {doc}");
    assert!(cs.iter().all(|candidate| candidate["original"] == "A"));
    assert!(cs.iter().all(|candidate| candidate["symbol"] == "choose"));
    assert!(
        compiled.iter().all(|output| output.status.success()),
        "{source} {compiled:?}"
    );
    assert_eq!(doc["candidates"], replay["candidates"]);
}

#[rstest::rstest]
#[case::unshadowed_function("def f(): return Status.A\n", 1)]
#[case::parameter_shadow("def f(Status): return Status.A\n", 0)]
#[case::local_assignment_shadow("def f():\n    x=Status.A\n    Status=object()\n", 0)]
#[case::module_assignment_shadow("Status=object()\nx=Status.A\n", 0)]
#[case::annotation("x: Status.A\n", 0)]
#[case::match_pattern("match x:\n    case Status.A: pass\n", 0)]
#[case::member_assignment("Status.A=1\n", 0)]
#[case::comprehension_shadow("x=[Status.A for Status in values]\n", 0)]
#[case::global_assignment_shadow(
    "def f():\n    global Status\n    Status=object()\nx=Status.A\n",
    0
)]
#[tokio::test]
async fn enum_scope_selects_only_unshadowed_load_references(
    #[case] tail: &str,
    #[case] count: usize,
) {
    let root = tempfile::tempdir().unwrap();
    let source = format!("import enum as e\nclass Status(e.Enum):\n    A=1; B=2\n{tail}");
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "enum_member_replace", "pass").await;

    assert_eq!(
        doc["candidates"].as_array().unwrap().len(),
        count,
        "{tail} {doc}"
    );
}

#[rstest::rstest]
#[case::mixed_manual_and_auto_values("A=1; B=auto()")]
#[case::ignored_member("_ignore_='B'; A=1; B=2")]
#[case::dynamic_member_value("A=1; B=make()")]
#[case::custom_auto_generator("A=1; B=2\n    def _generate_next_value_(*args): return 1")]
#[tokio::test]
async fn enum_unsupported_definitions_are_skipped_with_diagnostic(#[case] body: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("subject.py"),
        format!("from enum import Enum, auto\nclass Status(Enum):\n    {body}\nx=Status.A\n"),
    )
    .unwrap();

    let doc = plan(root.path(), "enum_member_replace", "pass").await;

    assert!(doc["candidates"].as_array().unwrap().is_empty(), "{doc}");
    assert!(doc.to_string().contains("enum_definition_skipped"), "{doc}");
}

#[rstest::rstest]
#[case::type_check_survives(
    "from subject import choose,Status; assert isinstance(choose(), Status)",
    0,
    2
)]
#[case::identity_check_kills(
    "from subject import choose,Status; assert choose() is Status.A",
    2,
    0
)]
#[tokio::test]
async fn enum_identity_checks_kill_what_type_checks_miss(
    #[case] test: &str,
    #[case] killed: usize,
    #[case] survived: usize,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), "from enum import Enum\nclass Status(Enum):\n    A=1; B=2; C=3\ndef choose():\n    return Status.A\n").unwrap();
    let doc = plan(root.path(), "enum_member_replace", test).await;
    let manifest = root.path().join("plan.json");
    std::fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();

    let (_, report) = cli(vec![
        "hoimin".into(),
        "verify".into(),
        manifest.into_os_string(),
        "--top".into(),
        "2".into(),
    ])
    .await;

    assert_eq!(report["baseline"]["termination"]["Exit"], 0);
    assert_eq!(report["summary"]["counts"]["killed"], killed);
    assert_eq!(report["summary"]["counts"]["survived"], survived);
}

#[rstest::rstest]
#[case::integer_literal_alias("Enum", "A=0x1; Alias=+1; B=2; C=3")]
#[case::case_folded_auto_alias("StrEnum", "A=auto(); a=auto(); B=auto(); C=auto()")]
#[tokio::test]
async fn enum_load_contexts_preserve_comments_and_match_cpython_values(
    #[case] base: &str,
    #[case] values: &str,
) {
    let root = tempfile::tempdir().unwrap();
    let source = format!(
        "from enum import {base}, auto\r\nclass Status({base}):\r\n    {values}\r\n    def method(self): return self.value\r\n# é前\r\nx=Status.A\r\ny=Status.A == x\r\nz=str(Status.A)\r\ndef choose():\r\n    return (Status # comment\r\n            . A)\r\n"
    );
    std::fs::write(root.path().join("subject.py"), &source).unwrap();

    let doc = plan(root.path(), "enum_member_replace", "pass").await;

    let mut executed = Vec::new();
    let mut preserved_prefixes = Vec::new();
    for c in doc["candidates"].as_array().unwrap() {
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let mut mutant = source.clone();
        mutant.replace_range(start..=start, c["replacement"].as_str().unwrap());
        let output = tokio::process::Command::new(python())
            .args([
                "-c",
                "import sys; ns={}; exec(compile(sys.argv[1],'<enum>','exec'),ns); S=ns['Status']; assert len(S)==3; assert S[sys.argv[2]] is not S.A; assert type(S[sys.argv[2]]) is S",
                &mutant,
                c["replacement"].as_str().unwrap(),
            ])
            .output()
            .await
            .unwrap();
        executed.push(output);
        preserved_prefixes.push(mutant[..start] == source[..start]);
    }

    assert_eq!(doc["candidates"].as_array().unwrap().len(), 8, "{doc}");
    assert!(
        executed.iter().all(|output| output.status.success()),
        "{executed:?}"
    );
    assert!(preserved_prefixes.into_iter().all(|preserved| preserved));
}

#[rstest::rstest]
#[case::annotated_module_alias(
    "import enum\nother: object=enum\nother.Enum=object\nclass E(enum.Enum): A=1; B=2\nx=E.A\n"
)]
#[case::walrus_module_alias(
    "import enum\n(other:=enum)\nother.Enum=object\nclass E(enum.Enum): A=1; B=2\nx=E.A\n"
)]
#[case::unpacked_module_alias(
    "import enum\nother,=(enum,)\nother.Enum=object\nclass E(enum.Enum): A=1; B=2\nx=E.A\n"
)]
#[case::deleted_global_enum(
    "from enum import Enum\nclass E(Enum): A=1; B=2\ndef remove():\n    global E\n    del E\nx=E.A\n"
)]
#[case::module_dictionary_replacement(
    "import enum\nclass Fake: pass\nenum.__dict__['Enum']=Fake\nclass E(enum.Enum): A=1; B=2\nx=E.A\n"
)]
#[case::module_attribute_replacement(
    "import enum\nother=enum\nclass Fake: pass\nother.Enum=Fake\nclass E(enum.Enum): A=1; B=2\nx=E.A\n"
)]
#[case::surrogate_member_values(
    "from enum import Enum\nclass E(Enum): A='\\ud800'; B='\\ud801'; C='\\ufffd'\nx=E.A\n"
)]
#[tokio::test]
async fn enum_review_regressions_reject_ambiguous_identity(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "enum_member_replace", "pass").await;

    assert!(doc["candidates"].as_array().unwrap().is_empty(), "{doc}");
    assert!(doc.to_string().contains("enum_definition_skipped"), "{doc}");
}

#[tokio::test]
async fn enum_review_regression_preserves_unicode_member_spelling() {
    let root = tempfile::tempdir().unwrap();
    let source = "from enum import Enum\nclass E(Enum): A=1; ｆｏｒ=2\nx=E.A\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "enum_member_replace", "pass").await;
    let compiled = compile(&source.replace("x=E.A", "x=E.ｆｏｒ")).await;

    assert_eq!(doc["candidates"][0]["replacement"], "ｆｏｒ");
    assert!(compiled.status.success(), "{compiled:?}");
}
