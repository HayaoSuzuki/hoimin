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
async fn stdlib_conversions_preserve_argument_source_and_compile() {
    for (import, callable) in [
        ("from decimal import Decimal", "Decimal"),
        ("from decimal import Decimal as D", "D"),
        ("import decimal", "decimal.Decimal"),
        ("import decimal as d", "d.Decimal"),
        ("from fractions import Fraction", "Fraction"),
        ("from fractions import Fraction as F", "F"),
        ("import fractions", "fractions.Fraction"),
        ("import fractions as f", "f.Fraction"),
    ] {
        let source = format!(
            "{import}\r\né = 1\r\ndef convert(raw):\r\n    return {callable}(\r\n        raw # retained argument\r\n    )\r\n"
        );
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let doc = plan(root.path(), "stdlib_conversion_call_remove", "pass").await;
        let rows = doc["candidates"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "{source}");
        assert_eq!(rows[0]["replacement"], "(raw)");
        let start = usize::try_from(rows[0]["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(rows[0]["span"]["length"].as_u64().unwrap()).unwrap();
        assert_eq!(&source[start..end], rows[0]["original"].as_str().unwrap());
        let mut mutant = source.clone();
        mutant.replace_range(start..end, rows[0]["replacement"].as_str().unwrap());
        compile(&mutant).await;
    }
}

#[tokio::test]
async fn stdlib_conversions_reject_uncertain_identities_and_unsafe_roles() {
    for source in [
        "def f(raw): return Decimal(raw)\n",
        "from other import Decimal\ndef f(raw): return Decimal(raw)\n",
        "import decimal\nx = decimal.Context(1)\n",
        "from fractions import Decimal\nx = Decimal(raw)\n",
        "import fractions\nx = fractions.Decimal(raw)\n",
        "from .decimal import Decimal\ndef f(raw): return Decimal(raw)\n",
        "from decimal import Decimal\nDecimal = custom\nx = Decimal(raw)\n",
        "from fractions import Fraction\ndef f(Fraction): return Fraction(raw)\n",
        "from decimal import Decimal\ndef f(raw):\n    Decimal = custom\n    return Decimal(raw)\n",
        "from decimal import Decimal\ndef rebind():\n    global Decimal\n    Decimal = custom\ndef f(raw): return Decimal(raw)\n",
        "def f(raw):\n    from decimal import Decimal\n    return Decimal(raw)\n",
        "import decimal\ndecimal.Decimal = custom\nx = decimal.Decimal(raw)\n",
        "import fractions as f\nf.Fraction = custom\nx = f.Fraction(raw)\n",
        "import decimal\nother = decimal\nx = decimal.Decimal(raw)\n",
        "import decimal\nconfigure(decimal)\nx = decimal.Decimal(raw)\n",
        "from decimal import Decimal\nfrom other import *\nx = Decimal(raw)\n",
        "from decimal import Decimal\nexec(code)\nx = Decimal(raw)\n",
        "from decimal import Decimal\nclass C:\n    x = Decimal(raw)\n",
        "from decimal import Decimal\nx = Decimal()\nx = Decimal(raw, context=ctx)\nx = Decimal(value=raw)\nx = Decimal(*args)\nx = Decimal(**kw)\n",
        "from fractions import Fraction\nx = Fraction(numerator, denominator)\n",
        "from decimal import Decimal\nx = Decimal(x for x in xs)\nx = Decimal((x:=1))\n",
        "from decimal import Decimal\nasync def f(): return Decimal(await get())\n",
        "from decimal import Decimal\ndef f(): return Decimal((yield 1))\n",
        "from decimal import Decimal\nfrom typing import TypeAlias\nA: TypeAlias = Decimal(raw)\nx: Decimal(raw)\ntype B = Decimal(raw)\na[Decimal(raw)] = 1\n",
        "from decimal import Decimal\nfrom typing import cast\nx = cast(Decimal, raw)\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "stdlib_conversion_call_remove", "pass").await;
        assert!(doc["candidates"].as_array().unwrap().is_empty(), "{source}");
    }
}

#[tokio::test]
async fn stdlib_conversion_value_equality_survives_but_type_assertion_kills() {
    for (module, class) in [("decimal", "Decimal"), ("fractions", "Fraction")] {
        for (check, killed, survived) in [
            (format!("assert result.amount == {class}('1.5')"), 0, 1),
            (
                format!(
                    "assert result.amount == {class}('1.5'); assert isinstance(result.amount, {class})"
                ),
                1,
                0,
            ),
        ] {
            let root = tempfile::tempdir().unwrap();
            let source = format!(
                "from {module} import {class}\nfrom dataclasses import dataclass\n@dataclass\nclass Result:\n    amount: {class}\ndef serialize(value):\n    return Result({class}(value))\n"
            );
            std::fs::write(root.path().join("subject.py"), source).unwrap();
            let test =
                format!("from subject import serialize, {class}; result=serialize(1.5); {check}");
            let doc = plan(root.path(), "stdlib_conversion_call_remove", &test).await;
            assert_eq!(doc["candidates"].as_array().unwrap().len(), 1);
            let manifest = root.path().join("plan.json");
            std::fs::write(&manifest, serde_json::to_vec(&doc).unwrap()).unwrap();
            let (_, report) = cli(vec![
                "hoimin".into(),
                "verify".into(),
                manifest.into_os_string(),
                "--top".into(),
                "1".into(),
                "--format".into(),
                "json".into(),
            ])
            .await;
            assert_eq!(report["baseline"]["termination"]["Exit"], 0);
            assert_eq!(report["summary"]["counts"]["killed"], killed);
            assert_eq!(report["summary"]["counts"]["survived"], survived);
        }
    }
}

#[tokio::test]
async fn stdlib_conversion_keeps_exactly_one_argument_evaluation() {
    for (module, class) in [("decimal", "Decimal"), ("fractions", "Fraction")] {
        let source = format!(
            "from {module} import {class}\ncalls=[]\ndef get():\n    calls.append('argument')\n    return 1.5\nresult = {class}(get())\n"
        );
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let doc = plan(root.path(), "stdlib_conversion_call_remove", "pass").await;
        let c = &doc["candidates"][0];
        assert_eq!(doc["candidates"].as_array().unwrap().len(), 1);
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source;
        mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
        mutant
            .push_str("assert calls == ['argument'] and type(result) is float and result == 1.5\n");
        let output = tokio::process::Command::new(python())
            .args(["-B", "-c", &mutant])
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(24))]

    #[test]
    fn stdlib_alias_candidates_keep_prefix_and_rebinding_excludes_them(
        alias in "convert_[a-z]{1,6}",
        import in proptest::sample::select(vec![("decimal", "Decimal"), ("fractions", "Fraction")]),
        count in 1_usize..9,
        limit in 1_usize..9,
    ) {
        use hoimin_cli::analyzer::discover_targets;
        use hoimin_core::{MutationOperatorSelection, MutationProfile, TargetSlice};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("subject.py");
        let root_path = camino::Utf8Path::from_path(root.path()).unwrap();
        let calls = (0..count).map(|i| format!("{alias}(value + {i})")).collect::<Vec<_>>().join(", ");
        let source = format!("from {} import {} as {alias}\ndef f(value):\n    return [{calls}]\n", import.0, import.1);
        std::fs::write(&path, &source).unwrap();
        let operators: MutationOperatorSelection = serde_json::from_str("[\"stdlib_conversion_call_remove\"]").unwrap();
        let targets = [TargetSlice { path: "subject.py".into(), lines: Vec::new(), symbols: Vec::new() }];
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let full = runtime.block_on(discover_targets(root_path, &targets, &operators, MutationProfile::Full, 16)).unwrap();
        let prefix = runtime.block_on(discover_targets(root_path, &targets, &operators, MutationProfile::Full, limit)).unwrap();
        proptest::prop_assert_eq!(full.candidates.len(), count);
        let replacements = full.candidates.iter().map(|c| c.replacement.clone()).collect::<Vec<_>>();
        let expected = (0..count).map(|i| format!("(value + {i})")).collect::<Vec<_>>();
        proptest::prop_assert_eq!(replacements, expected);
        proptest::prop_assert_eq!(&prefix.candidates, &full.candidates[..count.min(limit)]);
        proptest::prop_assert_eq!(prefix.truncated, count > limit);
        std::fs::write(&path, format!("{source}\n{alias} = custom\n")).unwrap();
        let shadowed = runtime.block_on(discover_targets(root_path, &targets, &operators, MutationProfile::Full, 16)).unwrap();
        proptest::prop_assert!(shadowed.candidates.is_empty());
    }
}

#[tokio::test]
async fn stdlib_conversions_resolve_both_modules_in_one_file() {
    let root = tempfile::tempdir().unwrap();
    let source = "import decimal as d\nfrom fractions import Fraction as F\ndef f(raw):\n    return d.Decimal(raw), F(raw)\n";
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let doc = plan(root.path(), "stdlib_conversion_call_remove", "pass").await;
    let candidates = doc["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0]["original"], "d.Decimal(raw)");
    assert_eq!(candidates[1]["original"], "F(raw)");
    assert_eq!(candidates[0]["replacement"], "(raw)");
    assert_eq!(candidates[1]["replacement"], "(raw)");
}
