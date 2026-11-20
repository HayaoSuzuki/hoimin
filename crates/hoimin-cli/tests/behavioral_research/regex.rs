use super::{assert_python_output, fs, plan, run_python};
use rstest::rstest;

const INPUTS: &str = "['123','１２３','١٢٣','1２3','²','é','_','\\u0301','🙂',' ','\\t','\\n','\\r','\\f','\\v','\\x1c','\\xa0']";
const CASES: &str = "['i','I','İ','ı','s','ſ','k','\u{212a}','é','É']";
const CLASS: &str = "regex_character_set";
const ROOT: &str = "regex_ascii_mode";
const BOUND: &str = "regex_match_boundary";

#[rstest]
#[case::digits(
    r"\d+",
    CLASS,
    "[0-9]",
    INPUTS,
    "11110000000000000",
    "10000000000000000"
)]
#[case::negative_digits(
    r"\D+",
    CLASS,
    "[^0-9]",
    INPUTS,
    "00001111111111111",
    "01101111111111111"
)]
#[case::word(
    r"\w+",
    CLASS,
    "[A-Za-z0-9_]",
    INPUTS,
    "11111110000000000",
    "10000010000000000"
)]
#[case::negative_word(
    r"\W+",
    CLASS,
    "[^A-Za-z0-9_]",
    INPUTS,
    "00000001111111111",
    "01101101111111111"
)]
#[case::space(
    r"\s+",
    CLASS,
    r"[\x09-\x0d\x20]",
    INPUTS,
    "00000000011111111",
    "00000000011111100"
)]
#[case::negative_space(
    r"\S+",
    CLASS,
    r"[^\x09-\x0d\x20]",
    INPUTS,
    "11111111100000000",
    "11111111100000011"
)]
#[case::positive_ascii_class(
    "[3-90-2]+",
    CLASS,
    r"\d",
    INPUTS,
    "10000000000000000",
    "11110000000000000"
)]
#[case::negative_ascii_class(
    "[^3-90-2]+",
    CLASS,
    r"\D",
    INPUTS,
    "01101111111111111",
    "00001111111111111"
)]
#[case::fold_range("(?i)[a-z]", CLASS, "(?a:[a-z])", CASES, "1111111100", "1100101000")]
#[case::fold_literal("(?i)k", ROOT, "(?a)(", CASES, "0000001100", "0000001000")]
#[case::fold_accent(r"(?i)\u00e9", ROOT, "(?a)(", CASES, "0000000011", "0000000010")]
#[case::fold_reference(
    r"(?i)(.)\1",
    ROOT,
    "(?a)(",
    "['éÉ','\u{212a}k','ii','kk','éé']",
    "11111",
    "00111"
)]
#[case::mixed(r"[\d!]", CLASS, r"(?a:[\d!])", "['!','9','９','x']", "1110", "1100")]
#[case::scope_restoration(
    r"(?a:\d)(?u:\d)",
    CLASS,
    r"(?u:\d)",
    "['９９','9９','９9','99']",
    "0101",
    "1111"
)]
#[case::boundary(r"\b.", CLASS, r"(?a:\b)", "['é','a','!','']", "1100", "0100")]
#[case::not_boundary(r"\B.?", CLASS, r"(?a:\B)", "['é','!','a','']", "0101", "1101")]
fn regex_acceptance(
    #[case] pattern: &str,
    #[case] op: &str,
    #[case] replacement: &str,
    #[case] inputs: &str,
    #[case] before: &str,
    #[case] after: &str,
) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let source = format!("import re\np=re.compile(r'{pattern}')\n");
    let harness = format!(
        "from subject import p\nprint(''.join('1' if p.fullmatch(x) else '0' for x in {inputs}))"
    );
    runtime.block_on(assert_outcome(
        &source,
        op,
        replacement,
        &harness,
        &format!("{before}\n"),
        &format!("{after}\n"),
    ));
}

#[rstest]
#[case::compiled_region(
    "import re\ndef run(s):\n p=re.compile('(a)')\n return p.search(s,1,4)\n",
    "match",
    "from subject import run\nm=run('xxa!'); print((m.span(),m.groups()) if m else None)",
    "((2, 3), ('a',))\n",
    "None\n"
)]
#[case::final_newline(
    "import re\np=re.compile(r'a$')\n",
    r"\Z",
    "from subject import p\nm=p.search('a\\n'); print(m.span() if m else None)",
    "(0, 1)\n",
    "None\n"
)]
#[case::multiline(
    "import re\np=re.compile(rb'^a',re.M)\n",
    r"\A",
    "from subject import p\nm=p.search(b'\\xff\\na'); print(m.span() if m else None)",
    "(2, 3)\n",
    "None\n"
)]
#[case::compiled_leading_anchor(
    "import re\np=re.compile(r'^a')\n",
    "(?:)",
    "from subject import p\nm=p.match('xa',1); print(m.span() if m else None)",
    "None\n",
    "(1, 2)\n"
)]
#[case::method_span(
    "import re\nfrom re import fullmatch as f\ndef run(s): return f('(a)',s)\n",
    "re.match",
    "from subject import run\nm=run('a!'); print((m.span(),m.groups()) if m else None)",
    "None\n",
    "((0, 1), ('a',))\n"
)]
#[tokio::test]
async fn boundary_span(
    #[case] source: &str,
    #[case] dest: &str,
    #[case] harness: &str,
    #[case] before: &str,
    #[case] after: &str,
) {
    assert_outcome(source, BOUND, dest, harness, before, after).await;
}
#[rstest]
#[case::normal_escape(
    "import re\np=re.compile('\\\\' # retained\n  'd+')\n",
    "[0-9]' # retained\n  '",
    "１２",
    "True\n",
    "False\n"
)]
#[case::complement(
    "import re\np=re.compile('\\\\' # retained\n  'D+')\n",
    "[^0-9]' # retained\n  '",
    "１２",
    "False\n",
    "True\n"
)]
#[case::raw_class(
    "import re\np=re.compile(r'[0-' # retained\n  r'9]+')\n",
    "\\d' # retained\n  r'",
    "１２",
    "False\n",
    "True\n"
)]
#[case::boundary(
    "import re\np=re.compile('\\\\' # retained\n  'b.')\n",
    "(?a:\\\\' # retained\n  'b)",
    "é",
    "True\n",
    "False\n"
)]
#[case::local_literal(
    "import re\ndef make():\n p=r'\\w+'\n return re.compile(p)\np=make()\n",
    "r'[A-Za-z0-9_]+'",
    "é",
    "True\n",
    "False\n"
)]
#[tokio::test]
async fn decoded_source(
    #[case] source: &str,
    #[case] replacement: &str,
    #[case] subject: &str,
    #[case] before: &str,
    #[case] after: &str,
) {
    let harness = format!("from subject import p\nprint(p.fullmatch('{subject}') is not None)");
    assert_outcome(source, CLASS, replacement, &harness, before, after).await;
}

async fn assert_outcome(
    source: &str,
    op: &str,
    dest: &str,
    harness: &str,
    before: &str,
    after: &str,
) {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("subject.py");
    fs::write(&file, source).unwrap();

    let manifest = plan(root.path(), op).await;
    let baseline = run_python(root.path(), harness);
    let mut observed = None;
    for candidate in manifest["candidates"].as_array().unwrap() {
        let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
        assert_eq!(Some(&source[start..end]), candidate["original"].as_str());
        let new = candidate["replacement"].as_str().unwrap();
        let mut changed = source.to_owned();
        changed.replace_range(start..end, new);
        fs::write(&file, changed).unwrap();
        let output = run_python(root.path(), harness);
        assert!(
            output.status.success(),
            "{new:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if new == dest {
            observed = Some(output);
        }
    }

    assert_python_output(op, "original", &baseline, before);
    let output = observed.unwrap_or_else(|| panic!("missing {dest:?} in {manifest}"));
    assert_python_output(op, "mutant", &output, after);
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(12))]
    #[test]
    fn cross_token_digit_classes_match_independent_decimal_oracle(split in 1usize..5,negate in proptest::bool::ANY, raw_second in proptest::bool::ANY){
        let class=if negate{"[^0-9]"}else{"[0-9]"};
        let split=split.min(class.len()-1);
        let source=format!("import re\ndef build():\n value=(r'{}' # boundary\n  {}'{}')\n compiled=re.compile(value)\n return compiled\np=build()\n",&class[..split],if raw_second{"r"}else{""},&class[split..]);
        let root=tempfile::tempdir().unwrap();
        let path=root.path().join("subject.py");
        fs::write(&path,&source).unwrap();
        let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

        let manifest=runtime.block_on(plan(root.path(),CLASS));
        let candidates=manifest["candidates"].as_array().unwrap();
        proptest::prop_assert_eq!(candidates.len(),1);
        let c=&candidates[0];
        let start=usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end=start+usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut changed=source.clone();
        changed.replace_range(start..end,c["replacement"].as_str().unwrap());
        fs::write(path,changed).unwrap();
        let checker=format!("from subject import p\nchars='x9９١²é_🙂'\nassert p.pattern==r'{}'\nassert all(bool(p.fullmatch(x))==({}x.isdecimal()) for x in chars)",if negate{r"\D"}else{r"\d"},if negate{"not "}else{""});
        let output=run_python(root.path(),&checker);

        proptest::prop_assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    }
}

#[rstest]
#[case::room_for_import(199, 1)]
#[case::at_source_limit(200, 0)]
#[tokio::test]
async fn aliased_match_import_at_source_depth(#[case] depth: usize, #[case] expected: usize) {
    let source = format!(
        "from re import search as f\nout={}f{}('a','ba')\n",
        "(".repeat(depth),
        ")".repeat(depth)
    );
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("subject.py");
    fs::write(&path, &source).unwrap();

    let manifest = plan(root.path(), BOUND).await;
    let original = run_python(root.path(), "import subject");
    let candidates = manifest["candidates"].as_array().unwrap();
    for c in candidates {
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut changed = source.clone();
        changed.replace_range(start..end, c["replacement"].as_str().unwrap());
        fs::write(&path, changed).unwrap();
        assert_python_output(
            BOUND,
            "mutated",
            &run_python(root.path(), "import subject"),
            "",
        );
    }

    assert_python_output(BOUND, "original", &original, "");
    assert_eq!(candidates.len(), expected);
}

#[rstest]
#[case::external_flags(
    "from re import compile, I, A as local\np=compile(r'\\d',flags=I|local)\n",
    ROOT,
    "32",
    "from subject import p\nprint(p.flags,bool(p.fullmatch('９')))",
    "258 False\n",
    "34 True\n"
)]
#[case::global_and_scoped(
    "import re\np=re.compile(r'(?i)(?a:\\w)\\w')\n",
    ROOT,
    "(?a)(",
    "from subject import p\nprint(p.flags,bool(p.fullmatch('Aé')),bool(p.fullmatch('éA')))",
    "34 True False\n",
    "258 False False\n"
)]
#[case::re_flags_observable(
    "import re\nm=re.fullmatch(r'\\d','3')\n",
    ROOT,
    "(?a)\\",
    "from subject import m\nprint(m.span(),m.re.flags)",
    "(0, 1) 32\n",
    "(0, 1) 256\n"
)]
#[case::per_atom_keeps_flags(
    "import re\nm=re.fullmatch(r'\\d','3')\n",
    CLASS,
    "[0-9]",
    "from subject import m\nprint(m.span(),m.re.flags)",
    "(0, 1) 32\n",
    "(0, 1) 32\n"
)]
#[case::disable_verbose(
    "import re\np=re.compile(r'''(?x)# ignored \\d\n(?-x:#\\w)''')\n",
    CLASS,
    "[A-Za-z0-9_]",
    "from subject import p\nprint(bool(p.fullmatch('#é')))",
    "True\n",
    "False\n"
)]
#[case::unicode_case_under_ascii(
    "import re\np=re.compile(r'(?u:[a-z])', re.ASCII|re.I)\n",
    CLASS,
    "(?a:[a-z])",
    "from subject import p\nprint(bool(p.fullmatch('\u{212a}')))",
    "True\n",
    "False\n"
)]
#[case::ignorecase_removal(
    "import re\np=re.compile(r'(?i)(?-i:\\W)')\n",
    CLASS,
    "[^A-Za-z0-9_]",
    "from subject import p\nprint(bool(p.fullmatch('\u{212a}')))",
    "False\n",
    "True\n"
)]
#[case::ordinary_escape(
    "import re\np=re.compile('\\x5cd+')\n",
    CLASS,
    "[0-9]",
    "from subject import p\nprint(bool(p.fullmatch('９')))",
    "True\n",
    "False\n"
)]
#[case::physical_continuation(
    "import re\np=re.compile('\\\\\\\nd')\n",
    CLASS,
    "[0-9]\\\n",
    "from subject import p\nprint(bool(p.fullmatch('９')))",
    "True\n",
    "False\n"
)]
#[tokio::test]
async fn effective_mode_and_literal_spelling(
    #[case] source: &str,
    #[case] op: &str,
    #[case] replacement: &str,
    #[case] harness: &str,
    #[case] before: &str,
    #[case] after: &str,
) {
    assert_outcome(source, op, replacement, harness, before, after).await;
}

#[tokio::test]
async fn crossing_raw_backslash_parity_preserves_the_boundary() {
    let source = "import re\np=re.compile('\\\\' r'\\\\' # crossing\n 'b.')\n";
    let harness = "from subject import p\nprint(bool(p.fullmatch('\\\\é')))";

    assert_outcome(
        source,
        CLASS,
        "(?a:\\b)' # crossing\n '",
        harness,
        "True\n",
        "False\n",
    )
    .await;
}

#[rstest]
#[case::module("import re\ndef run(): return re.prefixmatch('(a)','a!')\n")]
#[case::compiled("import re\ndef run(): return re.compile('(a)').prefixmatch('a!')\n")]
#[tokio::test]
async fn python315_prefix_spelling_uses_cross_version_destinations(#[case] source: &str) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("subject.py");
    fs::write(&path, source).unwrap();
    let harness = "from subject import run\ntry: m=run(); print((m.span(),m.groups()) if m else None)\nexcept AttributeError: print('unavailable')";

    let manifest = plan(root.path(), BOUND).await;
    let original = run_python(root.path(), harness);
    let version = run_python(root.path(), "import sys; print(sys.version_info.minor)");
    let baseline = if String::from_utf8_lossy(&version.stdout).trim() == "14" {
        "unavailable\n"
    } else {
        "((0, 1), ('a',))\n"
    };
    let candidates = manifest["candidates"].as_array().unwrap();
    for c in candidates {
        let start = usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end = start + usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let dest = c["replacement"].as_str().unwrap();
        let expected = match dest {
            "search" => "((0, 1), ('a',))\n",
            "fullmatch" => "None\n",
            other => panic!("unexpected destination {other}"),
        };
        let mut changed = source.to_owned();
        changed.replace_range(start..end, dest);
        fs::write(&path, changed).unwrap();
        assert_python_output(
            BOUND,
            "changed",
            &run_python(root.path(), harness),
            expected,
        );
    }

    assert_eq!(candidates.len(), 2);
    assert_python_output(BOUND, "original", &original, baseline);
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(8))]
    #[test]
    fn generated_capture_and_reference_follow_effective_decimal_mode(
        count in 1usize..4, external_ascii in proptest::bool::ANY, override_mode in proptest::bool::ANY
    ){
        let ascii=external_ascii^override_mode;
        let atom=if override_mode{if ascii{r"(?a:\d)"}else{r"(?u:\d)"}}else{r"\d"};
        let source=format!("import re\np=re.compile(r'\\A(?P<num>{atom}{{{count}}})(?P=num)\\Z',{})\n",if external_ascii{"re.A"}else{"0"});
        let root=tempfile::tempdir().unwrap();
        let path=root.path().join("subject.py");fs::write(&path,&source).unwrap();
        let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let harness=|ascii:bool|format!("from subject import p\nfor digit in '2９١x':\n piece=digit*{count}\n for tail in (piece,'0'*{count}):\n  m=p.fullmatch(piece+tail)\n  assert bool(m)==(digit.isdecimal() and (digit.isascii() or {}) and piece==tail)\n  if m: assert m.groups()==(piece,) and m.span()==(0,2*{count})",if ascii{"False"}else{"True"});

        let manifest=runtime.block_on(plan(root.path(),CLASS));
        let original=run_python(root.path(),&harness(ascii));
        let candidates=manifest["candidates"].as_array().unwrap();
        proptest::prop_assert_eq!(candidates.len(),1);
        let c=&candidates[0];
        let start=usize::try_from(c["span"]["start"].as_u64().unwrap()).unwrap();
        let end=start+usize::try_from(c["span"]["length"].as_u64().unwrap()).unwrap();
        let mut changed=source.clone();changed.replace_range(start..end,c["replacement"].as_str().unwrap());
        fs::write(path,changed).unwrap();
        let mutant=run_python(root.path(),&harness(!ascii));

        proptest::prop_assert!(original.status.success(),"{}",String::from_utf8_lossy(&original.stderr));
        proptest::prop_assert!(mutant.status.success(),"{}",String::from_utf8_lossy(&mutant.stderr));
    }
}

#[rstest]
#[case::bytes_pattern("re.search(rb'x$', f'{value}')", 0, true)]
#[case::bytes_subject("re.search(f'{value}', b'x')", 0, true)]
#[case::compiled_bytes("re.compile(b'x').search(f'{value}')", 0, true)]
#[case::compiled_text("re.compile(f'{value}').search(b'x')", 0, true)]
#[case::dynamic_text("re.search(f'{value}', 'xx')", 1, false)]
#[case::compiled_dynamic("re.compile(f'{value}').search('xx')", 1, false)]
#[tokio::test]
async fn fix1_match_type_contract(
    #[case] expression: &str,
    #[case] count: usize,
    #[case] mismatch: bool,
) {
    let root = tempfile::tempdir().unwrap();
    let source = format!("import re\nvalue='x'\ndef run(): return {expression}\n");
    fs::write(root.path().join("subject.py"), source).unwrap();
    let harness =
        "from subject import run\ntry: print(run().span())\nexcept TypeError: print('TypeError')";

    let manifest = plan(root.path(), BOUND).await;
    let output = run_python(root.path(), harness);

    assert_eq!(manifest["candidates"].as_array().unwrap().len(), count);
    assert_python_output(
        BOUND,
        "original",
        &output,
        if mismatch { "TypeError\n" } else { "(0, 1)\n" },
    );
}

#[rstest]
#[case::last_valid(4300, true)]
#[case::first_invalid(4301, false)]
#[tokio::test]
async fn fix1_conditional_conversion_contract(#[case] digits: usize, #[case] valid: bool) {
    let root = tempfile::tempdir().unwrap();
    let pattern = format!(r"(\d)(?({}1)y|n)", "0".repeat(digits - 1));
    let source = format!("import re\np=re.compile(r'{pattern}')\n");
    fs::write(root.path().join("subject.py"), source).unwrap();

    let manifest = plan(root.path(), CLASS).await;
    let output = run_python(
        root.path(),
        "try:\n from subject import p\n print(p.fullmatch('1y').groups())\nexcept ValueError: print('ValueError')",
    );

    assert_eq!(
        manifest["candidates"].as_array().unwrap().len(),
        usize::from(valid)
    );
    assert_python_output(
        CLASS,
        "original",
        &output,
        if valid { "('1',)\n" } else { "ValueError\n" },
    );
}

#[rstest]
#[case::universal(r"[\x00-\U0010ffff]", true, true, 0)]
#[case::empty_complement(r"[^\U00010000-\U0010ffff\x00-\uffff]", false, false, 0)]
#[case::unsorted_split(r"[\x80-\U0010ffff\x00-\x7f]", true, true, 0)]
#[case::missing_kelvin(r"[\x00-\u2129\u212b-\U0010ffff]", true, false, 2)]
#[case::complement_missing_kelvin(r"[^\x00-\u2129\u212b-\U0010ffff]", false, true, 2)]
#[case::categories_cover_domain(r"[\D\w]", true, true, 0)]
#[case::ranges_and_categories(r"[^\D0-9\x80-\U0010ffff]", false, false, 0)]
#[tokio::test]
async fn fix1_union_mode_contract(
    #[case] class: &str,
    #[case] unicode: bool,
    #[case] ascii: bool,
    #[case] count: usize,
) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("subject.py");
    let source = format!("import re\np=re.compile(r'(?i){class}')\n");
    fs::write(&path, &source).unwrap();
    let harness = "from subject import p\nprint(bool(p.fullmatch(chr(0x212a))))";
    let expected = |v| if v { "True\n" } else { "False\n" };

    let original = run_python(root.path(), harness);
    let manifest = plan(root.path(), "regex_ascii_mode,regex_character_set").await;
    let candidates = manifest["candidates"].as_array().unwrap();

    assert_python_output(CLASS, "original", &original, expected(unicode));
    assert_eq!(candidates.len(), count);
    for candidate in candidates {
        let start = usize::try_from(candidate["span"]["start"].as_u64().unwrap()).unwrap();
        let length = usize::try_from(candidate["span"]["length"].as_u64().unwrap()).unwrap();
        let mut changed = source.clone();
        changed.replace_range(
            start..start + length,
            candidate["replacement"].as_str().unwrap(),
        );
        fs::write(&path, changed).unwrap();
        assert_python_output(
            CLASS,
            "mutant",
            &run_python(root.path(), harness),
            expected(ascii),
        );
    }
}

#[rstest]
#[case::positive_atom("", CLASS, "100001\n", "111111\n")]
#[case::negative_atom("^", CLASS, "011110\n", "000000\n")]
#[case::positive_root("", ROOT, "100001\n", "111111\n")]
#[case::negative_root("^", ROOT, "011110\n", "000000\n")]
#[tokio::test]
async fn fix2_whitespace_union_contract(
    #[case] sign: &str,
    #[case] op: &str,
    #[case] original: &str,
    #[case] mutant: &str,
) {
    let class = format!(r"[{sign}\S\x09-\x0d\x20\x80-\U0010ffff]");
    let source = format!("import re\np=re.compile(r'(?u){class}')\n");
    let scoped = format!("(?a:{class})");
    let replacement = if op == CLASS { &scoped } else { "a" };
    let harness = "from subject import p\nprint(''.join(str(int(bool(p.fullmatch(chr(n))))) for n in range(0x1b,0x21)))";

    assert_outcome(&source, op, replacement, harness, original, mutant).await;
}
