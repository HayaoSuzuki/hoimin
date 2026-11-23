#[path = "common/stdlib_witness.rs"]
mod witness;
use witness::{assert_python, mutant};

#[rstest::rstest]
#[case::direct_alias("from urllib.parse import quote_plus as qp", "qp")]
#[case::submodule_alias("from urllib import parse as p", "p.quote_plus")]
#[case::package_root("import urllib.parse", "urllib.parse.quote_plus")]
#[tokio::test]
async fn final_review_quote_without_import_builtin(#[case] setup: &str, #[case] spelling: &str) {
    // Arrange
    let source = format!(
        "{setup}\nfrom urllib.parse import quote as q\n__import__ = None\nout = q('a b/c')"
    );
    assert_python(&source, "assert out == 'a%20b/c'").await;
    // Act
    let mutated = mutant(&source, "url_quote_swap", spelling);
    // Assert
    assert_python(&mutated, "assert out == 'a+b%2Fc'").await;
}

#[rstest::rstest]
#[case::member("from urllib.parse import quote_plus as _C__dst")]
#[case::module("import urllib.parse as _C__dst")]
#[tokio::test]
async fn class_private_destination_uses_safe_fallback(#[case] setup: &str) {
    // Arrange
    let source = format!(
        "from urllib.parse import quote as q\n{setup}\nclass C:\n    def quote_text(self, __dst):\n        return q('a b/c')\nout = C().quote_text(lambda text: 'wrong callable')"
    );
    assert_python(&source, "assert out == 'a%20b/c'").await;

    // Act
    let mutated = mutant(
        &source,
        "url_quote_swap",
        "__import__('urllib.parse').parse.quote_plus",
    );

    // Assert
    assert_python(&mutated, "assert out == 'a+b%2Fc'").await;
}

#[rstest::rstest]
#[case::map(
    "map_strict_toggle",
    "",
    "list(map(int.__add__, [1,2], [10]))",
    "[11]",
    "ValueError"
)]
#[case::batched(
    "batched_strict_toggle",
    "from itertools import batched",
    "list(batched(iterable=[1,2,3], n=2))",
    "[(1,2),(3,)]",
    "ValueError"
)]
#[case::quote(
    "url_quote_swap",
    "from urllib.parse import quote as q",
    "q('a b/c')",
    "'a%20b/c'",
    "'a+b%2Fc'"
)]
#[case::unquote(
    "url_quote_swap",
    "import urllib.parse",
    "urllib.parse.unquote('a+b')",
    "'a+b'",
    "'a b'"
)]
#[case::urlencode(
    "url_query_policy",
    "import urllib.parse as p",
    "p.urlencode({'a':[1,2]})",
    "'a=%5B1%2C+2%5D'",
    "'a=1&a=2'"
)]
#[case::parse(
    "url_query_policy",
    "from urllib.parse import parse_qs",
    "parse_qs('a=&b=ok')",
    "{'b':['ok']}",
    "{'a':[''],'b':['ok']}"
)]
#[case::base64(
    "base64_validate_toggle",
    "from base64 import b64decode\nfrom binascii import Error",
    "b64decode('YQ==!')",
    "b'a'",
    "Error"
)]
#[case::json(
    "json_allow_nan_toggle",
    "import json",
    "json.dumps(float('nan'),cls=None)",
    "'NaN'",
    "ValueError"
)]
#[tokio::test]
async fn boundary(
    #[case] id: &str,
    #[case] imports: &str,
    #[case] expr: &str,
    #[case] before: &str,
    #[case] after: &str,
) {
    // Arrange
    let source = format!("{imports}\ntry: out = {expr}\nexcept Exception as err: out = type(err)");
    assert_python(&source, &format!("assert out == {before},out")).await;
    // Act
    let mutated = mutant(&source, id, "");
    // Assert
    assert_python(&mutated, &format!("assert out == {after},out")).await;
}

#[tokio::test]
async fn ignored_base64_chars_imply_strict_validation_on_python_315() {
    // Arrange
    let source = "import sys\nfrom base64 import b64decode\nfrom binascii import Error\nif sys.version_info >= (3,15):\n try: out = b64decode('YQ==!?',ignorechars=b'!')\n except Error: out = None";
    assert_python(source, "if sys.version_info >= (3,15): assert out is None").await;
    // Act
    let mutated = mutant(source, "base64_validate_toggle", "");
    // Assert
    assert_python(
        &mutated,
        "if sys.version_info >= (3,15): assert out == b'a',out",
    )
    .await;
}

#[rstest::rstest]
#[case::map(
    "map_strict_toggle",
    "",
    "map(int.__add__, [1], [2], strict=policy())",
    "['make','bool','returned']",
    "['make','bool','returned']"
)]
#[case::batched(
    "batched_strict_toggle",
    "from itertools import batched",
    "batched([1],1,strict=policy())",
    "['make','bool','returned']",
    "['make','bool','returned']"
)]
#[case::query_blanks(
    "url_query_policy",
    "from urllib.parse import parse_qsl",
    "parse_qsl('a=&b=',policy())",
    "['make','bool','bool','returned']",
    "['make','bool','bool','returned']"
)]
#[case::query_no_blanks(
    "url_query_policy",
    "from urllib.parse import parse_qsl",
    "parse_qsl('a=b',policy())",
    "['make','returned']",
    "['make','returned']"
)]
#[case::base64(
    "base64_validate_toggle",
    "from base64 import b64decode",
    "b64decode('YQ==',validate=policy())",
    "['make','bool','returned']",
    "['make','bool','returned']"
)]
#[case::json(
    "json_allow_nan_toggle",
    "from json import dumps",
    "dumps([1],allow_nan=policy())",
    "['make','bool','bool','returned']",
    "['make','bool','returned']"
)]
#[tokio::test]
async fn observes_native_truth_timing(
    #[case] id: &str,
    #[case] imports: &str,
    #[case] expr: &str,
    #[case] before: &str,
    #[case] after: &str,
) {
    // Arrange
    let source = format!(
        "{imports}\nevents=[]\nclass Flag:\n def __bool__(self): events.append('bool');return False\ndef policy(): events.append('make');return Flag()\nout={expr}\nevents.append('returned')\nlist(out)"
    );
    assert_python(&source, &format!("assert events == {before}, events")).await;
    // Act
    let mutated = mutant(&source, id, "");
    // Assert
    assert_python(&mutated, &format!("assert events == {after}, events")).await;
}

#[tokio::test]
async fn query_policy_keeps_truth_exception_identity() {
    // Arrange
    let source = "from urllib.parse import parse_qsl\nclass Flag:\n def __bool__(self): raise failure\nfailure=ValueError('flag')\ntry: parse_qsl('a=',Flag())\nexcept ValueError as e: retained=e";
    // Act
    let mutated = mutant(source, "url_query_policy", "");
    // Assert
    assert_python(&mutated, "assert retained is failure").await;
}

#[rstest::rstest]
#[case::quote("quote")]
#[case::unquote("unquote")]
#[tokio::test]
async fn unreserved_ascii_is_invariant(#[case] api: &str) {
    // Arrange
    let source = format!("import urllib.parse as p\ndef changed(s):return p.{api}(s)");
    let property = "from itertools import product\nfrom string import ascii_letters,digits\nfor pair in product(ascii_letters+digits+'_.-~',repeat=2):\n s=''.join(pair)\n assert changed(s)==s,s";
    assert_python(&source, property).await;
    // Act
    let mutated = mutant(&source, "url_quote_swap", "");
    // Assert
    assert_python(&mutated, property).await;
}
