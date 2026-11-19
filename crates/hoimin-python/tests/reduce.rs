use hoimin_python::reduce::DefinitionSource;

#[rstest::rstest]
#[case("@decorator\ndef test_a():\n    assert True\n", "")]
#[case(
    "class Tests:\n    async def test_a(self):\n        pass\n",
    "class Tests:\n    pass\n"
)]
#[case(
    "if True:\n    def test_a():\n        pass\nelse:\n    x = 1\n",
    "if True:\n    pass\nelse:\n    x = 1\n"
)]
#[case("def test_a(): pass\nx = 1\n", "x = 1\n")]
fn removes_whole_definition(#[case] source: &str, #[case] expected: &str) {
    // Arrange
    let definitions = DefinitionSource::parse(source.as_bytes()).unwrap();
    // Act
    let actual = definitions.remove(&[0], &[]).unwrap();
    // Assert
    assert_eq!(actual, expected.as_bytes());
}

#[test]
fn protects_nested_kept_definition() {
    // Arrange
    let definitions = DefinitionSource::parse(b"def outer():\n    def inner(): pass\n").unwrap();
    // Act
    let actual = definitions.remove(&[0], &[1]);
    // Assert
    assert!(actual.is_err());
}

#[test]
fn rejects_ambiguous_qualified_name() {
    // Arrange
    let definitions = DefinitionSource::parse(b"def test_a(): pass\ndef test_a(): pass\n").unwrap();
    // Act
    let actual = definitions.select("test_a");
    // Assert
    assert!(actual.is_err());
}

#[test]
fn preserves_original_latin1_and_crlf() {
    // Arrange
    let source = b"# coding: latin-1\r\nname = '\xe9'\r\ndef test_a(): pass\r\n";
    let definitions = DefinitionSource::parse(source).unwrap();
    // Act
    let actual = definitions.remove(&[0], &[]).unwrap();
    // Assert
    assert_eq!(actual, b"# coding: latin-1\r\nname = '\xe9'\r\n");
}

proptest::proptest! {
    #[test]
    fn removal_preserves_unselected_suffix(value in -100_000_i64..100_000) {
        // Arrange
        let suffix = format!("answer = {value}\n");
        let source = format!("def test_a(): pass\n{suffix}");
        let definitions = DefinitionSource::parse(source.as_bytes()).unwrap();
        // Act
        let actual = definitions.remove(&[0], &[]).unwrap();
        // Assert
        proptest::prop_assert_eq!(actual,suffix.as_bytes());
    }
}

#[test]
fn decorated_last_method_repairs_class_suite() {
    // Arrange
    let source =
        DefinitionSource::parse(b"class Tests:\n    @decorator\n    def test_a(self): pass\n")
            .unwrap();
    // Act
    let actual = source.remove(&[0], &[]);
    // Assert
    assert_eq!(actual.unwrap(), b"class Tests:\n    pass\n");
}

#[rstest::rstest]
#[case(
    "if flag:\n    def same(): pass\nelse:\n    def same(): pass\n",
    "same"
)]
#[case(
    "class Tests:\n    def same(self): pass\n    def same(self): pass\n",
    "Tests.same"
)]
fn conditional_or_redefined_name_is_ambiguous(#[case] source: &str, #[case] selector: &str) {
    // Arrange
    let definitions = DefinitionSource::parse(source.as_bytes()).unwrap();
    // Act
    let result = definitions.select(selector);
    // Assert
    assert!(result.is_err());
}

#[test]
fn removing_ancestor_normalizes_nested_edits() {
    // Arrange
    let source = DefinitionSource::parse(b"def outer():\n    def inner(): pass\n").unwrap();
    // Act
    let actual = source.remove(&[0, 1], &[]).unwrap();
    // Assert
    assert_eq!(actual, b"");
}

proptest::proptest! {
    #[test]
    fn removing_generated_method_preserves_other_method(value in -100_000_i64..100_000, indent in proptest::sample::select(vec!["    ","\t"]), newline in proptest::sample::select(vec!["\n","\r\n"])) {
        // Arrange
        let kept=format!("class Tests:{newline}{indent}def kept(self): return {value}{newline}");
        let source=format!("{kept}{indent}@decorator{newline}{indent}async def removed(self): pass{newline}");
        let definitions=DefinitionSource::parse(source.as_bytes()).unwrap();
        // Act
        let actual=definitions.remove(&[1],&[0]).unwrap();
        // Assert
        proptest::prop_assert_eq!(actual,kept.as_bytes());
    }
}

#[rstest::rstest]
#[case(b"def invalid(:")]
#[case(b"# coding: unsupported-codec\ndef valid(): pass\n")]
#[case(b"\xff")]
fn invalid_source_is_rejected_before_editing(#[case] bytes: &[u8]) {
    // Act
    let actual = DefinitionSource::parse(bytes);
    // Assert
    assert!(actual.is_err());
}
