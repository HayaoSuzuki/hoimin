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

async fn compile(source: &str) {
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(python())
            .args([
                "-c",
                "import sys; assert sys.version_info[:2] == (3, 14), sys.version; compile(sys.argv[1], '<neighbor>', 'exec')",
                source,
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("CPython compiler deadline")
    .expect("CPython 3.14; set HOIMIN_OPERATOR_TEST_PYTHON to its executable");
    assert!(
        output.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn cli(args: Vec<OsString>) -> (i32, serde_json::Value) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = hoimin_cli::run_with_io(args, &mut stdout, &mut stderr).await;
    assert!(
        code <= 1,
        "exit={code}, stderr={}",
        String::from_utf8_lossy(&stderr)
    );
    (
        code,
        serde_json::from_slice(&stdout).expect("one JSON document"),
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

#[tokio::test]
async fn enum_members_are_distinct_canonical_destinations() {
    for (base, members, expected) in [
        ("Enum", "A=1; B=2; C=3", vec!["B", "C"]),
        ("IntEnum", "A=-1; Alias=-1; B=2", vec!["B"]),
        ("Enum", "A='a'; Alias='a'; B='b'", vec!["B"]),
        ("Enum", "A=auto(); B=auto(); C=auto()", vec!["B", "C"]),
        ("StrEnum", "A=auto(); a=auto(); B=auto()", vec!["B"]),
        ("Enum", "A=1; Alias=1", vec![]),
    ] {
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
        assert_eq!(actual, expected, "{source} {doc}");
        for c in cs {
            assert_eq!(c["original"], "A");
            assert_eq!(c["symbol"], "choose");
            let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
            let mut mutant = source.clone();
            mutant.replace_range(start..=start, c["replacement"].as_str().unwrap());
            compile(&mutant).await;
        }
        assert_eq!(
            doc["candidates"],
            plan(root.path(), "enum_member_replace", "pass").await["candidates"]
        );
    }
}

#[tokio::test]
async fn enum_scope_and_unsupported_definitions_are_conservative() {
    for (tail, count) in [
        ("def f(): return Status.A\n", 1),
        ("def f(Status): return Status.A\n", 0),
        ("def f():\n    x=Status.A\n    Status=object()\n", 0),
        ("Status=object()\nx=Status.A\n", 0),
        ("x: Status.A\n", 0),
        ("match x:\n    case Status.A: pass\n", 0),
        ("Status.A=1\n", 0),
        ("x=[Status.A for Status in values]\n", 0),
        (
            "def f():\n    global Status\n    Status=object()\nx=Status.A\n",
            0,
        ),
    ] {
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
    for body in [
        "A=1; B=auto()",
        "_ignore_='B'; A=1; B=2",
        "A=1; B=make()",
        "A=1; B=2\n    def _generate_next_value_(*args): return 1",
    ] {
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
}

#[tokio::test]
async fn enum_identity_checks_kill_what_type_checks_miss() {
    for (test, killed, survived) in [
        (
            "from subject import choose,Status; assert isinstance(choose(), Status)",
            0,
            2,
        ),
        (
            "from subject import choose,Status; assert choose() is Status.A",
            2,
            0,
        ),
    ] {
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
            "--format".into(),
            "json".into(),
        ])
        .await;
        assert_eq!(report["baseline"]["termination"]["Exit"], 0);
        assert_eq!(report["summary"]["counts"]["killed"], killed);
        assert_eq!(report["summary"]["counts"]["survived"], survived);
    }
}

#[tokio::test]
async fn enum_load_contexts_preserve_comments_and_match_cpython_values() {
    for (base, values) in [
        ("Enum", "A=0x1; Alias=+1; B=2; C=3"),
        ("StrEnum", "A=auto(); a=auto(); B=auto(); C=auto()"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let source = format!(
            "from enum import {base}, auto\r\nclass Status({base}):\r\n    {values}\r\n    def method(self): return self.value\r\n# é前\r\nx=Status.A\r\ny=Status.A == x\r\nz=str(Status.A)\r\ndef choose():\r\n    return (Status # comment\r\n            . A)\r\n"
        );
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let doc = plan(root.path(), "enum_member_replace", "pass").await;
        assert_eq!(doc["candidates"].as_array().unwrap().len(), 8, "{doc}");
        for c in doc["candidates"].as_array().unwrap() {
            let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
            let mut mutant = source.clone();
            mutant.replace_range(start..=start, c["replacement"].as_str().unwrap());
            let output=tokio::process::Command::new(python()).args(["-c", "import sys; ns={}; exec(compile(sys.argv[1],'<enum>','exec'),ns); S=ns['Status']; assert len(S)==3; assert S[sys.argv[2]] is not S.A; assert type(S[sys.argv[2]]) is S", &mutant, c["replacement"].as_str().unwrap()]).output().await.unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(&mutant[..start], &source[..start]);
        }
    }
}

#[tokio::test]
async fn enum_review_regressions_reject_ambiguous_identity_and_preserve_spelling() {
    for source in [
        "import enum\nother: object=enum\nother.Enum=object\nclass E(enum.Enum): A=1; B=2\nx=E.A\n",
        "import enum\n(other:=enum)\nother.Enum=object\nclass E(enum.Enum): A=1; B=2\nx=E.A\n",
        "import enum\nother,=(enum,)\nother.Enum=object\nclass E(enum.Enum): A=1; B=2\nx=E.A\n",
        "from enum import Enum\nclass E(Enum): A=1; B=2\ndef remove():\n    global E\n    del E\nx=E.A\n",
        "import enum\nclass Fake: pass\nenum.__dict__['Enum']=Fake\nclass E(enum.Enum): A=1; B=2\nx=E.A\n",
        "import enum\nother=enum\nclass Fake: pass\nother.Enum=Fake\nclass E(enum.Enum): A=1; B=2\nx=E.A\n",
        "from enum import Enum\nclass E(Enum): A='\\ud800'; B='\\ud801'; C='\\ufffd'\nx=E.A\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "enum_member_replace", "pass").await;
        assert!(doc["candidates"].as_array().unwrap().is_empty(), "{doc}");
        assert!(doc.to_string().contains("enum_definition_skipped"), "{doc}");
    }
    let root = tempfile::tempdir().unwrap();
    let source = "from enum import Enum\nclass E(Enum): A=1; ｆｏｒ=2\nx=E.A\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "enum_member_replace", "pass").await;
    assert_eq!(doc["candidates"][0]["replacement"], "ｆｏｒ");
    compile(&source.replace("x=E.A", "x=E.ｆｏｒ")).await;
}
