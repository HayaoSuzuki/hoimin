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
async fn optional_keywords_cover_positions_and_compile() {
    let definition = "def render(text='x', mode='plain', *, escape=True, ending='!'):\n    return text, mode, escape, ending\n";
    for (call, count) in [
        ("render(escape=False)", 1),
        ("render(text='v', mode='raw', escape=False)", 3),
        ("render('v', mode='raw', escape=False, ending='.')", 3),
        (
            "render(\n text='v', # comment\n escape=(False),\n ending='.',\n)",
            3,
        ),
        ("(render)(escape=False)", 1),
    ] {
        let source = format!("é = 1\r\n{definition}value = {call}\n");
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), &source).unwrap();
        let doc = plan(root.path(), "optional_keyword_delete", "pass").await;
        let candidates = doc["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), count, "{source}");
        for c in candidates {
            let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
            let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
            let mut mutant = source.clone();
            mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
            compile(&mutant).await;
            mutant.push_str("assert type(value) is tuple and len(value)==4\n");
            let output = tokio::process::Command::new(python())
                .args(["-c", &mutant])
                .output()
                .await
                .unwrap();
            assert!(
                output.status.success(),
                "{mutant}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert_eq!(
            doc["candidates"],
            plan(root.path(), "optional_keyword_delete", "pass").await["candidates"]
        );
    }
}

#[tokio::test]
async fn optional_keywords_validate_complete_signature_binding() {
    for call in [
        "f(a=2)",
        "f(1,x=2,a=3)",
        "f(1,a=2,unknown=3)",
        "f(1,2,a=3)",
        "f(x=1,a=2)",
        "f(*args,a=2)",
        "f(1,**kw)",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("subject.py"),
            format!("def f(x, /, *, a=1): return x+a\n{call}\n"),
        )
        .unwrap();
        assert!(
            plan(root.path(), "optional_keyword_delete", "pass").await["candidates"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{call}"
        );
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("subject.py"), "def f(x, /, y=2, *, required, a=1): return x+y+a\nf(1,required=0,a=3)\nf(1,y=3,required=0,a=4)\n").unwrap();
    assert_eq!(
        plan(root.path(), "optional_keyword_delete", "pass").await["candidates"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn optional_keywords_exclude_ambiguous_and_unsafe_calls() {
    for source in [
        "def f(*args,a=1): pass\nf(a=2)\n",
        "def f(a=1,**kw): pass\nf(a=2)\n",
        "async def f(a=1): pass\nf(a=2)\n",
        "@decorator\ndef f(a=1): pass\nf(a=2)\n",
        "if flag:\n    def f(a=1): pass\nf(a=2)\n",
        "def f(a=1): pass\ndef f(a=1): pass\nf(a=2)\n",
        "def f(a=1): pass\nf = other\nf(a=2)\n",
        "from other import f\nf(a=2)\n",
        "def f(a=1): pass\nalias=f\nalias(a=2)\nf(a=2)\n",
        "def f(a=1): pass\ndef g(f): return f(a=2)\n",
        "def f(a=1): pass\ndef g():\n    f(a=2)\n    f=other\n",
        "def f(a=1): pass\ndef g():\n    global f\n    f=other\nf(a=2)\n",
        "def f(a=1): pass\nf.__defaults__=(3,)\nf(a=2)\n",
        "def f(a=1): pass\nsink(f)\nf(a=2)\n",
        "def f(a=1): pass\nexec(code)\nf(a=2)\n",
        "f(a=2)\ndef f(a=1): pass\n",
        "def f(a=1): pass\nf(a=(x:=2))\n",
        "def f(a=1): pass\nasync def g(): return f(a=await h())\n",
        "def f(a=1): pass\ndef g(): return f(a=(yield 2))\n",
        "from typing import TypeAlias\ndef f(a=1): pass\nA: TypeAlias = f(a=2)\nx: f(a=2)\ntype B = f(a=2)\ntarget[f(a=2)] = 1\n",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        assert!(
            plan(root.path(), "optional_keyword_delete", "pass").await["candidates"]
                .as_array()
                .unwrap()
                .is_empty(),
            "{source}"
        );
    }
}

#[tokio::test]
async fn optional_keywords_saved_plan_observes_explicit_option() {
    for (test, killed, survived) in [
        (
            "from subject import page; assert isinstance(page(),str)",
            0,
            1,
        ),
        ("from subject import page; assert page() == '<x>'", 1, 0),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), "def render(text, *, escape=True):\n    return text.replace('<','&lt;') if escape else text\ndef page(): return render('<x>',escape=False)\n").unwrap();
        let doc = plan(root.path(), "optional_keyword_delete", test).await;
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

#[tokio::test]
async fn optional_keywords_use_existing_default_and_keep_evaluation_order() {
    for (source, count, assertion) in [
        (
            "events=[]\ndef default():\n    events.append('default')\n    return True\ndef explicit():\n    events.append('argument')\n    return False\ndef f(escape=default()): return escape\nvalue=f(escape=explicit())\n",
            1,
            "assert events==['default'] and value is True\n",
        ),
        (
            "events=[]\ndef mark(n):\n    events.append(n)\n    return n\ndef f(x,a=0,b=0): return x,a,b\nvalue=f(mark(1),a=mark(2),b=mark(3))\n",
            2,
            "assert (events,value) in [([1,2],(1,2,0)),([1,3],(1,0,3))]\n",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "optional_keyword_delete", "pass").await;
        let candidates = doc["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), count);
        for c in candidates {
            let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
            let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
            let mut mutant = source.to_owned();
            mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
            mutant.push_str(assertion);
            let output = tokio::process::Command::new(python())
                .args(["-c", &mutant])
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
}

#[tokio::test]
async fn optional_keywords_preserve_keyword_adjacency_and_unicode_spelling() {
    for (source, assertion) in [
        (
            "def f(*, flag=True): return flag\ndef g(): return(f)(flag=False)\n",
            "assert g() is True\n",
        ),
        (
            "def f(*, flag=True): return flag\nif(f)(flag=False): value=1\nelse: value=0\n",
            "assert value==1\n",
        ),
        (
            "def f(*, flag=True): return flag\nwhile(f)(flag=False): value=1; break\nelse: value=0\n",
            "assert value==1\n",
        ),
        (
            "def 𝒊𝒇(*, 𝒇𝒐𝒓=True): return 𝒇𝒐𝒓\nvalue=𝒊𝒇(𝒇𝒐𝒓=False)\n",
            "assert value is True\n",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("subject.py"), source).unwrap();
        let doc = plan(root.path(), "optional_keyword_delete", "pass").await;
        assert_eq!(doc["candidates"].as_array().unwrap().len(), 1);
        let c = &doc["candidates"][0];
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut mutant = source.to_owned();
        mutant.replace_range(start..end, c["replacement"].as_str().unwrap());
        compile(&mutant).await;
        mutant.push_str(assertion);
        let output = tokio::process::Command::new(python())
            .args(["-c", &mutant])
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{mutant}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
