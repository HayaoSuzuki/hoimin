#[path = "support/collecting_writer.rs"]
mod collecting_writer;
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
        serde_json::from_slice(&stdout.snapshot()).expect("one JSON document"),
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
        import in proptest::sample::select(vec![
            ("decimal", "Decimal", ""), ("fractions", "Fraction", ""),
            ("decimal", "Decimal", ".from_float"), ("decimal", "Decimal", ".from_number"),
            ("fractions", "Fraction", ".from_float"), ("fractions", "Fraction", ".from_decimal"),
            ("fractions", "Fraction", ".from_number"), ("http", "HTTPStatus", ""),
            ("re", "RegexFlag", ""),
        ]),
        count in 1_usize..9,
        limit in 1_usize..9,
    ) {
        use hoimin_cli::analyzer::discover_targets;
        use hoimin_core::{MutationOperatorSelection, MutationProfile, TargetSlice};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("subject.py");
        let root_path = camino::Utf8Path::from_path(root.path()).unwrap();
        let calls = (0..count).map(|i| format!("{alias}{}(value + {i})", import.2)).collect::<Vec<_>>().join(", ");
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

const FACTORIES: &[(&str, &str, &str)] = &[
    ("decimal", "Decimal", "from_float"),
    ("decimal", "Decimal", "from_number"),
    ("fractions", "Fraction", "from_float"),
    ("fractions", "Fraction", "from_decimal"),
    ("fractions", "Fraction", "from_number"),
];

const INTEGER_ENUMS: &[(&str, &str)] = &[
    ("http", "HTTPStatus"),
    ("socket", "AddressFamily"),
    ("socket", "SocketKind"),
    ("socket", "MsgFlag"),
    ("socket", "AddressInfo"),
    ("signal", "Signals"),
    ("signal", "Handlers"),
    ("signal", "Sigmasks"),
    ("ssl", "VerifyMode"),
    ("ssl", "VerifyFlags"),
    ("ssl", "Options"),
    ("ssl", "AlertDescription"),
    ("ssl", "SSLErrorNumber"),
    ("ssl", "TLSVersion"),
    ("re", "RegexFlag"),
    ("calendar", "Day"),
    ("calendar", "Month"),
    ("inspect", "BufferFlags"),
    ("annotationlib", "Format"),
];

async fn assert_conversion_forms(module: &str, class: &str, suffix: &str) {
    for (import, name) in [
        (format!("from {module} import {class}"), class.to_owned()),
        (format!("from {module} import {class} as C"), "C".into()),
        (format!("import {module}"), format!("{module}.{class}")),
        (format!("import {module} as m"), format!("m.{class}")),
    ] {
        let call = format!("{name}{suffix}(raw)");
        let source = format!("{import}\r\né=1\r\ndef convert(raw):\r\n    return {call}\r\n");
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let doc = plan(root.path(), "stdlib_conversion_call_remove", "pass").await;
        let rows = doc["candidates"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "{source}");
        assert_eq!(rows[0]["original"], call);
        assert_eq!(rows[0]["replacement"], "(raw)");
        let start = usize::try_from(rows[0]["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(rows[0]["span"]["length"].as_u64().unwrap()).unwrap();
        assert_eq!(&source[start..end], call);
        let mut mutant = source;
        mutant.replace_range(start..end, "(raw)");
        compile(&mutant).await;
    }
}

#[tokio::test]
async fn stdlib_factory_methods_resolve_all_import_forms() {
    for &(module, class, method) in FACTORIES {
        assert_conversion_forms(module, class, &format!(".{method}")).await;
    }
}

#[tokio::test]
async fn concrete_integer_enums_resolve_all_import_forms() {
    for &(module, class) in INTEGER_ENUMS {
        assert_conversion_forms(module, class, "").await;
    }
}

#[tokio::test]
async fn factory_methods_reject_mutations_and_escaped_classes_across_aliases() {
    for source in [
        "from fractions import Fraction as F\nF.from_float = custom\nx = F.from_float(raw)\n",
        "import fractions as f\nf.Fraction.from_float = custom\nx = f.Fraction.from_float(raw)\n",
        "from fractions import Fraction as F\nalias = F\nx = F.from_float(raw)\n",
        "import fractions as f\nconfigure(f.Fraction)\nx = f.Fraction.from_float(raw)\n",
        "from fractions import Fraction\nclass C:\n    Fraction.from_float = custom\nx = Fraction.from_float(raw)\n",
        "def patch(): F.from_float = custom\nfrom fractions import Fraction as F\npatch()\nx = F.from_float(raw)\n",
        "from fractions import Fraction\nfrom fractions import Fraction as F\nfrom fractions import Fraction as F\nF.from_float = custom\nx = Fraction.from_float(raw)\n",
        "from fractions import Fraction\nfrom fractions import Fraction as F\nF.from_float = custom\nF = custom\nx = Fraction.from_float(raw)\n",
        "from fractions import Fraction\nbound = Fraction.from_float\nbound.__self__.from_float = custom\nx = Fraction.from_float(raw)\n",
        "from fractions import Fraction\ndel Fraction.from_float\nx = Fraction.from_float(raw)\n",
        "from fractions import Fraction\ndef patch():\n    from fractions import Fraction as F\n    F.from_float = custom\nx = Fraction.from_float(raw)\n",
        "from fractions import Fraction\nx: Fraction\ny = Fraction.from_float(raw)\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "stdlib_conversion_call_remove", "pass").await;
        assert!(doc["candidates"].as_array().unwrap().is_empty(), "{source}");
    }
}

#[tokio::test]
async fn extended_conversions_reject_unsupported_members_and_roles() {
    for source in [
        "from decimal import Decimal\nx = Decimal.from_decimal(raw)\n",
        "from fractions import Fraction\nx = Fraction.limit_denominator(raw)\n",
        "from fractions import Fraction\nx = Fraction.from_float()\nx = Fraction.from_float(raw, other)\nx = Fraction.from_float(f=raw)\nx = Fraction.from_float(*args)\n",
        "from fractions import Fraction\nx = Fraction.from_float((x:=1))\nx = Fraction.from_float(x for x in xs)\n",
        "from decimal import Decimal\nasync def f(): return Decimal.from_float(await get())\n",
        "from fractions import Fraction\nx: Fraction.from_float(raw)\ntype T = Fraction.from_float(raw)\na[Fraction.from_float(raw)] = 1\n",
        "from fractions import Fraction\nclass C:\n    x = Fraction.from_float(raw)\n",
        "from http import HTTPMethod\nx = HTTPMethod(raw)\n",
        "from ssl import Purpose\nx = Purpose(raw)\n",
        "from enum import IntEnum, IntFlag\nx = IntEnum(raw)\ny = IntFlag(raw)\n",
        "from enum import IntEnum\nclass Kind(IntEnum):\n    ONE = 1\nx = Kind(raw)\n",
        "from ctypes import c_int, c_double\nx = c_int(raw)\ny = c_double(raw)\n",
        "from http import HTTPStatus\nHTTPStatus = custom\nx = HTTPStatus(raw)\n",
        "import http\nhttp.HTTPStatus = custom\nx = http.HTTPStatus(raw)\n",
        "from calendar import Day\ndef f(Day): return Day(raw)\n",
        "from re import RegexFlag\nx = RegexFlag(value=raw)\n",
        "from http import HTTPStatus\nx: HTTPStatus(raw)\ntype T = HTTPStatus(raw)\na[HTTPStatus(raw)] = 1\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "stdlib_conversion_call_remove", "pass").await;
        assert!(doc["candidates"].as_array().unwrap().is_empty(), "{source}");
    }
}

async fn verify_conversion_type_and_evaluation(
    module: &str,
    class: &str,
    suffix: &str,
    setup: &str,
) {
    let source = format!(
        "from {module} import {class} as C\ncalls=[]\ndef get(value):\n    calls.append(1)\n    return value\ndef convert(raw):\n    return C{suffix}(get(raw))\n"
    );
    for typed in [false, true] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let assertion = if typed {
            "; assert isinstance(result, C)"
        } else {
            ""
        };
        let test = format!(
            "from subject import C, convert, calls; {setup}; result=convert(raw); assert calls == [1]; assert result == raw{assertion}"
        );
        let doc = plan(root.path(), "stdlib_conversion_call_remove", &test).await;
        assert_eq!(doc["candidates"].as_array().unwrap().len(), 1, "{source}");
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
        assert_eq!(
            report["baseline"]["termination"]["Exit"], 0,
            "{source}\n{test}\n{report}"
        );
        assert_eq!(
            report["summary"]["counts"]["killed"],
            u8::from(typed),
            "{report}"
        );
        assert_eq!(
            report["summary"]["counts"]["survived"],
            u8::from(!typed),
            "{report}"
        );
    }
}

#[tokio::test]
async fn factories_preserve_evaluation_but_need_type_assertions() {
    for &(module, class, method) in FACTORIES {
        let setup = if method == "from_decimal" {
            "from decimal import Decimal; raw=Decimal('1.5')"
        } else {
            "raw=1.5"
        };
        verify_conversion_type_and_evaluation(module, class, &format!(".{method}"), setup).await;
    }
}

#[tokio::test]
async fn integer_enums_preserve_evaluation_but_need_type_assertions() {
    for &(module, class) in INTEGER_ENUMS {
        // Sigmasks is documented as Unix-only. Its AST forms are still covered
        // on every platform by concrete_integer_enums_resolve_all_import_forms.
        if cfg!(windows) && (module, class) == ("signal", "Sigmasks") {
            continue;
        }
        verify_conversion_type_and_evaluation(
            module,
            class,
            "",
            "raw=int(next(iter(C.__members__.values())))",
        )
        .await;
    }
}

#[tokio::test]
async fn factories_keep_constructor_and_other_class_candidates() {
    for (source, originals) in [
        (
            "from fractions import Fraction as F\nx = F(raw)\ny = F.from_float(raw)\n",
            vec!["F(raw)", "F.from_float(raw)"],
        ),
        (
            "from decimal import Decimal as D\nfrom fractions import Fraction as F\nF.from_float = custom\nx = D.from_float(raw)\ny = F.from_float(raw)\nz = F(raw)\n",
            vec!["D.from_float(raw)", "F(raw)"],
        ),
        (
            "from decimal import Decimal\nx = Decimal(raw).from_float(raw)\n",
            vec!["Decimal(raw)"],
        ),
        (
            "from fractions import Fraction as F\nx = F(configure(F))\ny = F.from_float(raw)\n",
            vec!["F(configure(F))"],
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "stdlib_conversion_call_remove", "pass").await;
        let actual = doc["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["original"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual, originals, "{source}");
    }
}
