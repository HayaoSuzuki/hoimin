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

async fn compile(source: &str) {
    let output = tokio::time::timeout(
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
    .expect("CPython 3.14 or 3.15; set HOIMIN_OPERATOR_TEST_PYTHON to its executable");
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
#[case::from_decimal_import_decimal_1("from decimal import Decimal", "Decimal")]
#[case::from_decimal_import_decimal_as_d_2("from decimal import Decimal as D", "D")]
#[case::import_decimal_3("import decimal", "decimal.Decimal")]
#[case::import_decimal_as_d_4("import decimal as d", "d.Decimal")]
#[case::from_fractions_import_fraction_5("from fractions import Fraction", "Fraction")]
#[case::from_fractions_import_fraction_as_f_6("from fractions import Fraction as F", "F")]
#[case::import_fractions_7("import fractions", "fractions.Fraction")]
#[case::import_fractions_as_f_8("import fractions as f", "f.Fraction")]
#[tokio::test]
async fn stdlib_conversions_preserve_argument_source_and_compile(
    #[case] import: &str,
    #[case] callable: &str,
) {
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

#[rstest::rstest]
#[case::def_f_raw_return_decimal_raw_1("def f(raw): return Decimal(raw)\n")]
#[case::from_other_import_decimal_def_f_raw_return_decimal_raw_2(
    "from other import Decimal\ndef f(raw): return Decimal(raw)\n"
)]
#[case::import_decimal_x_decimal_context_1_3("import decimal\nx = decimal.Context(1)\n")]
#[case::from_fractions_import_decimal_x_decimal_raw_4(
    "from fractions import Decimal\nx = Decimal(raw)\n"
)]
#[case::import_fractions_x_fractions_decimal_raw_5(
    "import fractions\nx = fractions.Decimal(raw)\n"
)]
#[case::from_decimal_import_decimal_def_f_raw_return_decimal_ra_6(
    "from .decimal import Decimal\ndef f(raw): return Decimal(raw)\n"
)]
#[case::from_decimal_import_decimal_decimal_custom_x_decimal_ra_7(
    "from decimal import Decimal\nDecimal = custom\nx = Decimal(raw)\n"
)]
#[case::from_fractions_import_fraction_def_f_fraction_return_fr_8(
    "from fractions import Fraction\ndef f(Fraction): return Fraction(raw)\n"
)]
#[case::from_decimal_import_decimal_def_f_raw_decimal_custom_re_9(
    "from decimal import Decimal\ndef f(raw):\n    Decimal = custom\n    return Decimal(raw)\n"
)]
#[case::from_decimal_import_decimal_def_rebind_global_decimal_d_10(
    "from decimal import Decimal\ndef rebind():\n    global Decimal\n    Decimal = custom\ndef f(raw): return Decimal(raw)\n"
)]
#[case::def_f_raw_from_decimal_import_decimal_return_decimal_ra_11(
    "def f(raw):\n    from decimal import Decimal\n    return Decimal(raw)\n"
)]
#[case::import_decimal_decimal_decimal_custom_x_decimal_decimal_12(
    "import decimal\ndecimal.Decimal = custom\nx = decimal.Decimal(raw)\n"
)]
#[case::import_fractions_as_f_f_fraction_custom_x_f_fraction_ra_13(
    "import fractions as f\nf.Fraction = custom\nx = f.Fraction(raw)\n"
)]
#[case::import_decimal_other_decimal_x_decimal_decimal_raw_14(
    "import decimal\nother = decimal\nx = decimal.Decimal(raw)\n"
)]
#[case::import_decimal_configure_decimal_x_decimal_decimal_raw_15(
    "import decimal\nconfigure(decimal)\nx = decimal.Decimal(raw)\n"
)]
#[case::from_decimal_import_decimal_from_other_import_x_decimal_16(
    "from decimal import Decimal\nfrom other import *\nx = Decimal(raw)\n"
)]
#[case::from_decimal_import_decimal_exec_code_x_decimal_raw_17(
    "from decimal import Decimal\nexec(code)\nx = Decimal(raw)\n"
)]
#[case::from_decimal_import_decimal_class_c_x_decimal_raw_18(
    "from decimal import Decimal\nclass C:\n    x = Decimal(raw)\n"
)]
#[case::from_decimal_import_decimal_x_decimal_x_decimal_raw_con_19(
    "from decimal import Decimal\nx = Decimal()\nx = Decimal(raw, context=ctx)\nx = Decimal(value=raw)\nx = Decimal(*args)\nx = Decimal(**kw)\n"
)]
#[case::from_fractions_import_fraction_x_fraction_numerator_den_20(
    "from fractions import Fraction\nx = Fraction(numerator, denominator)\n"
)]
#[case::from_decimal_import_decimal_x_decimal_x_for_x_in_xs_x_d_21(
    "from decimal import Decimal\nx = Decimal(x for x in xs)\nx = Decimal((x:=1))\n"
)]
#[case::from_decimal_import_decimal_async_def_f_return_decimal_22(
    "from decimal import Decimal\nasync def f(): return Decimal(await get())\n"
)]
#[case::from_decimal_import_decimal_def_f_return_decimal_yield_23(
    "from decimal import Decimal\ndef f(): return Decimal((yield 1))\n"
)]
#[case::from_decimal_import_decimal_from_typing_import_typealia_24(
    "from decimal import Decimal\nfrom typing import TypeAlias\nA: TypeAlias = Decimal(raw)\nx: Decimal(raw)\ntype B = Decimal(raw)\na[Decimal(raw)] = 1\n"
)]
#[case::from_decimal_import_decimal_from_typing_import_cast_x_c_25(
    "from decimal import Decimal\nfrom typing import cast\nx = cast(Decimal, raw)\n"
)]
#[tokio::test]
async fn stdlib_conversions_reject_uncertain_identities_and_unsafe_roles(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "stdlib_conversion_call_remove", "pass").await;

    assert!(doc["candidates"].as_array().unwrap().is_empty(), "{source}");
}

#[rstest::rstest]
#[case::decimal_value_only("decimal", "Decimal", "assert result.amount == Decimal('1.5')", 0, 1)]
#[case::decimal_type_and_value(
    "decimal",
    "Decimal",
    "assert result.amount == Decimal('1.5'); assert isinstance(result.amount, Decimal)",
    1,
    0
)]
#[case::fractions_value_only(
    "fractions",
    "Fraction",
    "assert result.amount == Fraction('1.5')",
    0,
    1
)]
#[case::fractions_type_and_value(
    "fractions",
    "Fraction",
    "assert result.amount == Fraction('1.5'); assert isinstance(result.amount, Fraction)",
    1,
    0
)]
#[tokio::test]
async fn stdlib_conversion_value_equality_survives_but_type_assertion_kills(
    #[case] module: &str,
    #[case] class: &str,
    #[case] check: &str,
    #[case] killed: u8,
    #[case] survived: u8,
) {
    let root = tempfile::tempdir().unwrap();
    let source = format!(
        "from {module} import {class}\nfrom dataclasses import dataclass\n@dataclass\nclass Result:\n    amount: {class}\ndef serialize(value):\n    return Result({class}(value))\n"
    );
    std::fs::write(root.path().join("subject.py"), source).unwrap();
    let test = format!("from subject import serialize, {class}; result=serialize(1.5); {check}");

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
    ])
    .await;
    assert_eq!(report["baseline"]["termination"]["Exit"], 0);
    assert_eq!(report["summary"]["counts"]["killed"], killed);
    assert_eq!(report["summary"]["counts"]["survived"], survived);
}

#[rstest::rstest]
#[case::decimal_1("decimal", "Decimal")]
#[case::fractions_2("fractions", "Fraction")]
#[tokio::test]
async fn stdlib_conversion_keeps_exactly_one_argument_evaluation(
    #[case] module: &str,
    #[case] class: &str,
) {
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
    mutant.push_str("assert calls == ['argument'] and type(result) is float and result == 1.5\n");
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
        std::fs::write(&path, format!("{source}\n{alias} = custom\n")).unwrap();
        let shadowed = runtime.block_on(discover_targets(root_path, &targets, &operators, MutationProfile::Full, 16)).unwrap();
        let replacements = full.candidates.iter().map(|c| c.replacement.clone()).collect::<Vec<_>>();
        let expected = (0..count).map(|i| format!("(value + {i})")).collect::<Vec<_>>();

        proptest::prop_assert_eq!(full.candidates.len(), count);
        proptest::prop_assert_eq!(replacements, expected);
        proptest::prop_assert_eq!(&prefix.candidates, &full.candidates[..count.min(limit)]);
        proptest::prop_assert_eq!(prefix.truncated, count > limit);
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

#[rstest::rstest]
#[case::decimal_decimal_from_float_direct("from decimal import Decimal", "Decimal", ".from_float")]
#[case::decimal_decimal_from_float_class_alias(
    "from decimal import Decimal as C",
    "C",
    ".from_float"
)]
#[case::decimal_decimal_from_float_module("import decimal", "decimal.Decimal", ".from_float")]
#[case::decimal_decimal_from_float_module_alias("import decimal as m", "m.Decimal", ".from_float")]
#[case::decimal_decimal_from_number_direct(
    "from decimal import Decimal",
    "Decimal",
    ".from_number"
)]
#[case::decimal_decimal_from_number_class_alias(
    "from decimal import Decimal as C",
    "C",
    ".from_number"
)]
#[case::decimal_decimal_from_number_module("import decimal", "decimal.Decimal", ".from_number")]
#[case::decimal_decimal_from_number_module_alias(
    "import decimal as m",
    "m.Decimal",
    ".from_number"
)]
#[case::fractions_fraction_from_float_direct(
    "from fractions import Fraction",
    "Fraction",
    ".from_float"
)]
#[case::fractions_fraction_from_float_class_alias(
    "from fractions import Fraction as C",
    "C",
    ".from_float"
)]
#[case::fractions_fraction_from_float_module(
    "import fractions",
    "fractions.Fraction",
    ".from_float"
)]
#[case::fractions_fraction_from_float_module_alias(
    "import fractions as m",
    "m.Fraction",
    ".from_float"
)]
#[case::fractions_fraction_from_decimal_direct(
    "from fractions import Fraction",
    "Fraction",
    ".from_decimal"
)]
#[case::fractions_fraction_from_decimal_class_alias(
    "from fractions import Fraction as C",
    "C",
    ".from_decimal"
)]
#[case::fractions_fraction_from_decimal_module(
    "import fractions",
    "fractions.Fraction",
    ".from_decimal"
)]
#[case::fractions_fraction_from_decimal_module_alias(
    "import fractions as m",
    "m.Fraction",
    ".from_decimal"
)]
#[case::fractions_fraction_from_number_direct(
    "from fractions import Fraction",
    "Fraction",
    ".from_number"
)]
#[case::fractions_fraction_from_number_class_alias(
    "from fractions import Fraction as C",
    "C",
    ".from_number"
)]
#[case::fractions_fraction_from_number_module(
    "import fractions",
    "fractions.Fraction",
    ".from_number"
)]
#[case::fractions_fraction_from_number_module_alias(
    "import fractions as m",
    "m.Fraction",
    ".from_number"
)]
#[case::http_httpstatus_direct("from http import HTTPStatus", "HTTPStatus", "")]
#[case::http_httpstatus_class_alias("from http import HTTPStatus as C", "C", "")]
#[case::http_httpstatus_module("import http", "http.HTTPStatus", "")]
#[case::http_httpstatus_module_alias("import http as m", "m.HTTPStatus", "")]
#[case::socket_addressfamily_direct("from socket import AddressFamily", "AddressFamily", "")]
#[case::socket_addressfamily_class_alias("from socket import AddressFamily as C", "C", "")]
#[case::socket_addressfamily_module("import socket", "socket.AddressFamily", "")]
#[case::socket_addressfamily_module_alias("import socket as m", "m.AddressFamily", "")]
#[case::socket_socketkind_direct("from socket import SocketKind", "SocketKind", "")]
#[case::socket_socketkind_class_alias("from socket import SocketKind as C", "C", "")]
#[case::socket_socketkind_module("import socket", "socket.SocketKind", "")]
#[case::socket_socketkind_module_alias("import socket as m", "m.SocketKind", "")]
#[case::socket_msgflag_direct("from socket import MsgFlag", "MsgFlag", "")]
#[case::socket_msgflag_class_alias("from socket import MsgFlag as C", "C", "")]
#[case::socket_msgflag_module("import socket", "socket.MsgFlag", "")]
#[case::socket_msgflag_module_alias("import socket as m", "m.MsgFlag", "")]
#[case::socket_addressinfo_direct("from socket import AddressInfo", "AddressInfo", "")]
#[case::socket_addressinfo_class_alias("from socket import AddressInfo as C", "C", "")]
#[case::socket_addressinfo_module("import socket", "socket.AddressInfo", "")]
#[case::socket_addressinfo_module_alias("import socket as m", "m.AddressInfo", "")]
#[case::signal_signals_direct("from signal import Signals", "Signals", "")]
#[case::signal_signals_class_alias("from signal import Signals as C", "C", "")]
#[case::signal_signals_module("import signal", "signal.Signals", "")]
#[case::signal_signals_module_alias("import signal as m", "m.Signals", "")]
#[case::signal_handlers_direct("from signal import Handlers", "Handlers", "")]
#[case::signal_handlers_class_alias("from signal import Handlers as C", "C", "")]
#[case::signal_handlers_module("import signal", "signal.Handlers", "")]
#[case::signal_handlers_module_alias("import signal as m", "m.Handlers", "")]
#[case::signal_sigmasks_direct("from signal import Sigmasks", "Sigmasks", "")]
#[case::signal_sigmasks_class_alias("from signal import Sigmasks as C", "C", "")]
#[case::signal_sigmasks_module("import signal", "signal.Sigmasks", "")]
#[case::signal_sigmasks_module_alias("import signal as m", "m.Sigmasks", "")]
#[case::ssl_verifymode_direct("from ssl import VerifyMode", "VerifyMode", "")]
#[case::ssl_verifymode_class_alias("from ssl import VerifyMode as C", "C", "")]
#[case::ssl_verifymode_module("import ssl", "ssl.VerifyMode", "")]
#[case::ssl_verifymode_module_alias("import ssl as m", "m.VerifyMode", "")]
#[case::ssl_verifyflags_direct("from ssl import VerifyFlags", "VerifyFlags", "")]
#[case::ssl_verifyflags_class_alias("from ssl import VerifyFlags as C", "C", "")]
#[case::ssl_verifyflags_module("import ssl", "ssl.VerifyFlags", "")]
#[case::ssl_verifyflags_module_alias("import ssl as m", "m.VerifyFlags", "")]
#[case::ssl_options_direct("from ssl import Options", "Options", "")]
#[case::ssl_options_class_alias("from ssl import Options as C", "C", "")]
#[case::ssl_options_module("import ssl", "ssl.Options", "")]
#[case::ssl_options_module_alias("import ssl as m", "m.Options", "")]
#[case::ssl_alertdescription_direct("from ssl import AlertDescription", "AlertDescription", "")]
#[case::ssl_alertdescription_class_alias("from ssl import AlertDescription as C", "C", "")]
#[case::ssl_alertdescription_module("import ssl", "ssl.AlertDescription", "")]
#[case::ssl_alertdescription_module_alias("import ssl as m", "m.AlertDescription", "")]
#[case::ssl_sslerrornumber_direct("from ssl import SSLErrorNumber", "SSLErrorNumber", "")]
#[case::ssl_sslerrornumber_class_alias("from ssl import SSLErrorNumber as C", "C", "")]
#[case::ssl_sslerrornumber_module("import ssl", "ssl.SSLErrorNumber", "")]
#[case::ssl_sslerrornumber_module_alias("import ssl as m", "m.SSLErrorNumber", "")]
#[case::ssl_tlsversion_direct("from ssl import TLSVersion", "TLSVersion", "")]
#[case::ssl_tlsversion_class_alias("from ssl import TLSVersion as C", "C", "")]
#[case::ssl_tlsversion_module("import ssl", "ssl.TLSVersion", "")]
#[case::ssl_tlsversion_module_alias("import ssl as m", "m.TLSVersion", "")]
#[case::re_regexflag_direct("from re import RegexFlag", "RegexFlag", "")]
#[case::re_regexflag_class_alias("from re import RegexFlag as C", "C", "")]
#[case::re_regexflag_module("import re", "re.RegexFlag", "")]
#[case::re_regexflag_module_alias("import re as m", "m.RegexFlag", "")]
#[case::calendar_day_direct("from calendar import Day", "Day", "")]
#[case::calendar_day_class_alias("from calendar import Day as C", "C", "")]
#[case::calendar_day_module("import calendar", "calendar.Day", "")]
#[case::calendar_day_module_alias("import calendar as m", "m.Day", "")]
#[case::calendar_month_direct("from calendar import Month", "Month", "")]
#[case::calendar_month_class_alias("from calendar import Month as C", "C", "")]
#[case::calendar_month_module("import calendar", "calendar.Month", "")]
#[case::calendar_month_module_alias("import calendar as m", "m.Month", "")]
#[case::inspect_bufferflags_direct("from inspect import BufferFlags", "BufferFlags", "")]
#[case::inspect_bufferflags_class_alias("from inspect import BufferFlags as C", "C", "")]
#[case::inspect_bufferflags_module("import inspect", "inspect.BufferFlags", "")]
#[case::inspect_bufferflags_module_alias("import inspect as m", "m.BufferFlags", "")]
#[case::annotationlib_format_direct("from annotationlib import Format", "Format", "")]
#[case::annotationlib_format_class_alias("from annotationlib import Format as C", "C", "")]
#[case::annotationlib_format_module("import annotationlib", "annotationlib.Format", "")]
#[case::annotationlib_format_module_alias("import annotationlib as m", "m.Format", "")]
#[tokio::test]
async fn extended_stdlib_conversions_resolve_each_import_form(
    #[case] import: &str,
    #[case] name: &str,
    #[case] suffix: &str,
) {
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

#[rstest::rstest]
#[case::from_fractions_import_fraction_as_f_f_from_float_custom_1(
    "from fractions import Fraction as F\nF.from_float = custom\nx = F.from_float(raw)\n"
)]
#[case::import_fractions_as_f_f_fraction_from_float_custom_x_f_2(
    "import fractions as f\nf.Fraction.from_float = custom\nx = f.Fraction.from_float(raw)\n"
)]
#[case::from_fractions_import_fraction_as_f_alias_f_x_f_from_fl_3(
    "from fractions import Fraction as F\nalias = F\nx = F.from_float(raw)\n"
)]
#[case::import_fractions_as_f_configure_f_fraction_x_f_fraction_4(
    "import fractions as f\nconfigure(f.Fraction)\nx = f.Fraction.from_float(raw)\n"
)]
#[case::from_fractions_import_fraction_class_c_fraction_from_fl_5(
    "from fractions import Fraction\nclass C:\n    Fraction.from_float = custom\nx = Fraction.from_float(raw)\n"
)]
#[case::def_patch_f_from_float_custom_from_fractions_import_fra_6(
    "def patch(): F.from_float = custom\nfrom fractions import Fraction as F\npatch()\nx = F.from_float(raw)\n"
)]
#[case::from_fractions_import_fraction_from_fractions_import_fr_7(
    "from fractions import Fraction\nfrom fractions import Fraction as F\nfrom fractions import Fraction as F\nF.from_float = custom\nx = Fraction.from_float(raw)\n"
)]
#[case::from_fractions_import_fraction_from_fractions_import_fr_8(
    "from fractions import Fraction\nfrom fractions import Fraction as F\nF.from_float = custom\nF = custom\nx = Fraction.from_float(raw)\n"
)]
#[case::from_fractions_import_fraction_bound_fraction_from_floa_9(
    "from fractions import Fraction\nbound = Fraction.from_float\nbound.__self__.from_float = custom\nx = Fraction.from_float(raw)\n"
)]
#[case::from_fractions_import_fraction_del_fraction_from_float_10(
    "from fractions import Fraction\ndel Fraction.from_float\nx = Fraction.from_float(raw)\n"
)]
#[case::from_fractions_import_fraction_def_patch_from_fractions_11(
    "from fractions import Fraction\ndef patch():\n    from fractions import Fraction as F\n    F.from_float = custom\nx = Fraction.from_float(raw)\n"
)]
#[case::from_fractions_import_fraction_x_fraction_y_fraction_fr_12(
    "from fractions import Fraction\nx: Fraction\ny = Fraction.from_float(raw)\n"
)]
#[tokio::test]
async fn factory_methods_reject_mutations_and_escaped_classes_across_aliases(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "stdlib_conversion_call_remove", "pass").await;

    assert!(doc["candidates"].as_array().unwrap().is_empty(), "{source}");
}

#[rstest::rstest]
#[case::from_decimal_import_decimal_x_decimal_from_decimal_raw_1(
    "from decimal import Decimal\nx = Decimal.from_decimal(raw)\n"
)]
#[case::from_fractions_import_fraction_x_fraction_limit_denomin_2(
    "from fractions import Fraction\nx = Fraction.limit_denominator(raw)\n"
)]
#[case::from_fractions_import_fraction_x_fraction_from_float_x_3(
    "from fractions import Fraction\nx = Fraction.from_float()\nx = Fraction.from_float(raw, other)\nx = Fraction.from_float(f=raw)\nx = Fraction.from_float(*args)\n"
)]
#[case::from_fractions_import_fraction_x_fraction_from_float_x_4(
    "from fractions import Fraction\nx = Fraction.from_float((x:=1))\nx = Fraction.from_float(x for x in xs)\n"
)]
#[case::from_decimal_import_decimal_async_def_f_return_decimal_5(
    "from decimal import Decimal\nasync def f(): return Decimal.from_float(await get())\n"
)]
#[case::from_fractions_import_fraction_x_fraction_from_float_ra_6(
    "from fractions import Fraction\nx: Fraction.from_float(raw)\ntype T = Fraction.from_float(raw)\na[Fraction.from_float(raw)] = 1\n"
)]
#[case::from_fractions_import_fraction_class_c_x_fraction_from_7(
    "from fractions import Fraction\nclass C:\n    x = Fraction.from_float(raw)\n"
)]
#[case::from_http_import_httpmethod_x_httpmethod_raw_8(
    "from http import HTTPMethod\nx = HTTPMethod(raw)\n"
)]
#[case::from_ssl_import_purpose_x_purpose_raw_9("from ssl import Purpose\nx = Purpose(raw)\n")]
#[case::from_enum_import_intenum_intflag_x_intenum_raw_y_intfla_10(
    "from enum import IntEnum, IntFlag\nx = IntEnum(raw)\ny = IntFlag(raw)\n"
)]
#[case::from_enum_import_intenum_class_kind_intenum_one_1_x_kin_11(
    "from enum import IntEnum\nclass Kind(IntEnum):\n    ONE = 1\nx = Kind(raw)\n"
)]
#[case::from_ctypes_import_c_int_c_double_x_c_int_raw_y_c_doubl_12(
    "from ctypes import c_int, c_double\nx = c_int(raw)\ny = c_double(raw)\n"
)]
#[case::from_http_import_httpstatus_httpstatus_custom_x_httpsta_13(
    "from http import HTTPStatus\nHTTPStatus = custom\nx = HTTPStatus(raw)\n"
)]
#[case::import_http_http_httpstatus_custom_x_http_httpstatus_ra_14(
    "import http\nhttp.HTTPStatus = custom\nx = http.HTTPStatus(raw)\n"
)]
#[case::from_calendar_import_day_def_f_day_return_day_raw_15(
    "from calendar import Day\ndef f(Day): return Day(raw)\n"
)]
#[case::from_re_import_regexflag_x_regexflag_value_raw_16(
    "from re import RegexFlag\nx = RegexFlag(value=raw)\n"
)]
#[case::from_http_import_httpstatus_x_httpstatus_raw_type_t_htt_17(
    "from http import HTTPStatus\nx: HTTPStatus(raw)\ntype T = HTTPStatus(raw)\na[HTTPStatus(raw)] = 1\n"
)]
#[tokio::test]
async fn extended_conversions_reject_unsupported_members_and_roles(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), source).unwrap();

    let doc = plan(root.path(), "stdlib_conversion_call_remove", "pass").await;

    assert!(doc["candidates"].as_array().unwrap().is_empty(), "{source}");
}

#[rstest::rstest]
#[case::decimal_decimal_from_float_value_only(
    "decimal",
    "Decimal",
    ".from_float",
    "raw=1.5",
    "",
    0
)]
#[case::decimal_decimal_from_float_type_and_value(
    "decimal",
    "Decimal",
    ".from_float",
    "raw=1.5",
    "; assert isinstance(result, C)",
    1
)]
#[case::decimal_decimal_from_number_value_only(
    "decimal",
    "Decimal",
    ".from_number",
    "raw=1.5",
    "",
    0
)]
#[case::decimal_decimal_from_number_type_and_value(
    "decimal",
    "Decimal",
    ".from_number",
    "raw=1.5",
    "; assert isinstance(result, C)",
    1
)]
#[case::fractions_fraction_from_float_value_only(
    "fractions",
    "Fraction",
    ".from_float",
    "raw=1.5",
    "",
    0
)]
#[case::fractions_fraction_from_float_type_and_value(
    "fractions",
    "Fraction",
    ".from_float",
    "raw=1.5",
    "; assert isinstance(result, C)",
    1
)]
#[case::fractions_fraction_from_decimal_value_only(
    "fractions",
    "Fraction",
    ".from_decimal",
    "from decimal import Decimal; raw=Decimal('1.5')",
    "",
    0
)]
#[case::fractions_fraction_from_decimal_type_and_value(
    "fractions",
    "Fraction",
    ".from_decimal",
    "from decimal import Decimal; raw=Decimal('1.5')",
    "; assert isinstance(result, C)",
    1
)]
#[case::fractions_fraction_from_number_value_only(
    "fractions",
    "Fraction",
    ".from_number",
    "raw=1.5",
    "",
    0
)]
#[case::fractions_fraction_from_number_type_and_value(
    "fractions",
    "Fraction",
    ".from_number",
    "raw=1.5",
    "; assert isinstance(result, C)",
    1
)]
#[case::http_httpstatus_value_only(
    "http",
    "HTTPStatus",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::http_httpstatus_type_and_value(
    "http",
    "HTTPStatus",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::socket_addressfamily_value_only(
    "socket",
    "AddressFamily",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::socket_addressfamily_type_and_value(
    "socket",
    "AddressFamily",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::socket_socketkind_value_only(
    "socket",
    "SocketKind",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::socket_socketkind_type_and_value(
    "socket",
    "SocketKind",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::socket_msgflag_value_only(
    "socket",
    "MsgFlag",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::socket_msgflag_type_and_value(
    "socket",
    "MsgFlag",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::socket_addressinfo_value_only(
    "socket",
    "AddressInfo",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::socket_addressinfo_type_and_value(
    "socket",
    "AddressInfo",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::signal_signals_value_only(
    "signal",
    "Signals",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::signal_signals_type_and_value(
    "signal",
    "Signals",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::signal_handlers_value_only(
    "signal",
    "Handlers",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::signal_handlers_type_and_value(
    "signal",
    "Handlers",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[cfg_attr(
    not(windows),
    case::signal_sigmasks_value_only(
        "signal",
        "Sigmasks",
        "",
        "raw=int(next(iter(C.__members__.values())))",
        "",
        0
    )
)]
#[cfg_attr(
    not(windows),
    case::signal_sigmasks_type_and_value(
        "signal",
        "Sigmasks",
        "",
        "raw=int(next(iter(C.__members__.values())))",
        "; assert isinstance(result, C)",
        1
    )
)]
#[case::ssl_verifymode_value_only(
    "ssl",
    "VerifyMode",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::ssl_verifymode_type_and_value(
    "ssl",
    "VerifyMode",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::ssl_verifyflags_value_only(
    "ssl",
    "VerifyFlags",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::ssl_verifyflags_type_and_value(
    "ssl",
    "VerifyFlags",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::ssl_options_value_only(
    "ssl",
    "Options",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::ssl_options_type_and_value(
    "ssl",
    "Options",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::ssl_alertdescription_value_only(
    "ssl",
    "AlertDescription",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::ssl_alertdescription_type_and_value(
    "ssl",
    "AlertDescription",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::ssl_sslerrornumber_value_only(
    "ssl",
    "SSLErrorNumber",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::ssl_sslerrornumber_type_and_value(
    "ssl",
    "SSLErrorNumber",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::ssl_tlsversion_value_only(
    "ssl",
    "TLSVersion",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::ssl_tlsversion_type_and_value(
    "ssl",
    "TLSVersion",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::re_regexflag_value_only(
    "re",
    "RegexFlag",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::re_regexflag_type_and_value(
    "re",
    "RegexFlag",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::calendar_day_value_only(
    "calendar",
    "Day",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::calendar_day_type_and_value(
    "calendar",
    "Day",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::calendar_month_value_only(
    "calendar",
    "Month",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::calendar_month_type_and_value(
    "calendar",
    "Month",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::inspect_bufferflags_value_only(
    "inspect",
    "BufferFlags",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::inspect_bufferflags_type_and_value(
    "inspect",
    "BufferFlags",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[case::annotationlib_format_value_only(
    "annotationlib",
    "Format",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "",
    0
)]
#[case::annotationlib_format_type_and_value(
    "annotationlib",
    "Format",
    "",
    "raw=int(next(iter(C.__members__.values())))",
    "; assert isinstance(result, C)",
    1
)]
#[tokio::test]
async fn extended_conversions_preserve_evaluation_and_obey_type_checks(
    #[case] module: &str,
    #[case] class: &str,
    #[case] suffix: &str,
    #[case] setup: &str,
    #[case] assertion: &str,
    #[case] killed: u8,
) {
    let source = format!(
        "from {module} import {class} as C\ncalls=[]\ndef get(value):\n    calls.append(1)\n    return value\ndef convert(raw):\n    return C{suffix}(get(raw))\n"
    );

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), &source).unwrap();

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
    ])
    .await;
    assert_eq!(
        report["baseline"]["termination"]["Exit"], 0,
        "{source}\n{test}\n{report}"
    );
    assert_eq!(report["summary"]["counts"]["killed"], killed, "{report}");
    assert_eq!(
        report["summary"]["counts"]["survived"],
        1_u8 - killed,
        "{report}"
    );
}

#[rstest::rstest]
#[case::from_fractions_import_fraction_as_f_x_f_raw_y_f_from_fl_1("from fractions import Fraction as F\nx = F(raw)\ny = F.from_float(raw)\n", vec!["F(raw)", "F.from_float(raw)"])]
#[case::from_decimal_import_decimal_as_d_from_fractions_import_2("from decimal import Decimal as D\nfrom fractions import Fraction as F\nF.from_float = custom\nx = D.from_float(raw)\ny = F.from_float(raw)\nz = F(raw)\n", vec!["D.from_float(raw)", "F(raw)"])]
#[case::from_decimal_import_decimal_x_decimal_raw_from_float_ra_3("from decimal import Decimal\nx = Decimal(raw).from_float(raw)\n", vec!["Decimal(raw)"])]
#[case::from_fractions_import_fraction_as_f_x_f_configure_f_y_f_4("from fractions import Fraction as F\nx = F(configure(F))\ny = F.from_float(raw)\n", vec!["F(configure(F))"])]
#[tokio::test]
async fn factories_keep_constructor_and_other_class_candidates(
    #[case] source: &str,
    #[case] originals: Vec<&str>,
) {
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
