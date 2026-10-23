#[path = "support/collecting_writer.rs"]
mod collecting_writer;
#[path = "support/jsonl_report.rs"]
mod jsonl_report;
use collecting_writer::CollectingWriter;
use hoimin_cli::plan::PlanManifest;
use std::ffi::OsString;
use std::fmt::Write as _;

// Each parameter instance runs one candidate through plan, run and verify.
// The same-line identifier exercises Unicode scalar columns; the preceding
// comment exercises raw offsets. ASCII, UTF8, BOM and Latin1 stay distinct.
macro_rules! encoding_cases {
    ($($name:ident: $encoding:literal, $ending:literal;)*) => {$ (
        #[tokio::test]
        async fn $name() { flow($encoding, $ending).await; }
    )*};
}
encoding_cases! {
    ascii_lf: "ascii", "\n"; ascii_crlf: "ascii", "\r\n"; ascii_cr: "ascii", "\r";
    utf8_lf: "utf8", "\n"; utf8_crlf: "utf8", "\r\n"; utf8_cr: "utf8", "\r";
    bom_lf: "bom", "\n"; bom_crlf: "bom", "\r\n"; bom_cr: "bom", "\r";
    latin1_lf: "latin1", "\n"; latin1_crlf: "latin1", "\r\n"; latin1_cr: "latin1", "\r";
}

async fn flow(encoding: &str, ending: &str) {
    let project = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let (cookie, comment, name) = match encoding {
        "ascii" => ("# coding: ascii", "# plain", "cafe"),
        "latin1" => ("# coding: latin-1", "# café", "café"),
        _ => ("# coding: utf-8", "# café😀", "café"),
    };
    let text = format!(
        "{}{cookie}{ending}{comment}{ending}{name} = True{ending}",
        if encoding == "bom" { "\u{feff}" } else { "" }
    );
    let source: Vec<u8> = if encoding == "latin1" {
        text.chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap())
            .collect()
    } else {
        text.into_bytes()
    };
    let start = source.windows(4).position(|w| w == b"True").unwrap();
    let mut mutated = source[..start].to_vec();
    mutated.extend_from_slice(b"False");
    mutated.extend_from_slice(&source[start + 4..]);
    std::fs::write(project.path().join("calc.py"), &source).unwrap();
    let log = output.path().join("worker-bytes.txt");
    let python = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
        "../../.venv/Scripts/python.exe"
    } else {
        "../../.venv/bin/python"
    });
    let args: Vec<OsString> = vec![
        "hoimin".into(), "plan".into(), "--root".into(), project.path().into(),
        "--file".into(), "calc.py".into(), "--operators".into(), "boolean_literal".into(),
        "--max-candidates".into(), "1".into(), "--jobs".into(), "1".into(),
        "--allow-best-effort-memory".into(), "--min-free-space".into(), "1B".into(),
        "--".into(), python.into_os_string(), "-c".into(),
        format!("from pathlib import Path; Path({:?}).open('a').write(Path('calc.py').read_bytes().hex() + '\\n')", log.to_string_lossy()).into(),
    ];
    let stdout = CollectingWriter::default();
    let stderr = CollectingWriter::default();

    let code = hoimin_cli::run_with_writers(args.clone(), stdout.clone(), stderr.clone()).await;

    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
    let manifest: PlanManifest = jsonl_report::from_slice(&stdout.snapshot()).unwrap();
    assert_eq!(manifest.candidates.len(), 1);
    let candidate = &manifest.candidates[0];
    assert_eq!(
        (candidate.span.start, candidate.span.length),
        (u64::try_from(start).unwrap(), 4)
    );
    assert_eq!((candidate.line, candidate.column), (3, 7));
    assert_eq!(
        (&*candidate.original, &*candidate.replacement),
        ("True", "False")
    );
    assert_eq!(candidate.file_hash, blake3::hash(&source).to_hex().as_str());
    let plan = output.path().join("plan.json");
    std::fs::write(&plan, stdout.snapshot()).unwrap();
    let mut run = args;
    run[1] = "run".into();
    let verify = vec![
        "hoimin".into(),
        "verify".into(),
        plan.into_os_string(),
        "--top".into(),
        "1".into(),
    ];
    for command in [run, verify] {
        stdout.clear();
        stderr.clear();

        let code = hoimin_cli::run_with_writers(command, stdout.clone(), stderr.clone()).await;

        assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr.snapshot()));
        let report: serde_json::Value = jsonl_report::from_slice(&stdout.snapshot()).unwrap();
        let mutants = report["mutants"].as_array().unwrap();
        assert_eq!(mutants.len(), 1);
        assert_eq!(mutants[0]["candidate"]["id"], candidate.id);
        let observed = std::fs::read_to_string(&log).unwrap();
        let hex = |bytes: &[u8]| {
            bytes.iter().fold(String::new(), |mut output, byte| {
                write!(output, "{byte:02x}").unwrap();
                output
            })
        };
        assert_eq!(
            observed.lines().collect::<Vec<_>>(),
            [hex(&source), hex(&mutated)]
        );
        assert_eq!(
            std::fs::read(project.path().join("calc.py")).unwrap(),
            source
        );
        std::fs::remove_file(&log).unwrap();
    }
}

#[tokio::test]
async fn decoding_error_precedes_invalid_python_syntax() {
    let project = tempfile::tempdir().unwrap();
    let root = camino::Utf8Path::from_path(project.path()).unwrap();
    std::fs::write(root.join("bad.py"), b"# coding: ascii\n\xff = (\n").unwrap();
    let target = hoimin_core::TargetSlice {
        path: "bad.py".into(),
        lines: vec![],
        symbols: vec![],
    };

    let result = hoimin_cli::analyzer::discover_targets(
        root,
        &[target],
        &hoimin_core::MutationOperatorSelection::default(),
        hoimin_core::MutationProfile::Full,
        1,
    )
    .await;

    assert_eq!(
        result.unwrap_err().failure.code(),
        "analyzer.source.encoding"
    );
}
