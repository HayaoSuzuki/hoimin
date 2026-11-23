#[path = "common/stdlib_witness.rs"]
mod witness;
use witness::{assert_python, mutant};

#[rstest::rstest]
#[case::fullwidth("unicode_digit_ascii", "out = '２'.isdigit()", "", "True", "False")]
#[case::superscript(
    "unicode_digit_predicate_swap",
    "out = '²'.isdecimal()",
    "isdigit",
    "False",
    "True"
)]
#[case::fraction(
    "unicode_digit_predicate_swap",
    "out = '½'.isdigit()",
    "isnumeric",
    "False",
    "True"
)]
#[case::compatibility(
    "unicode_normalization_form",
    "import unicodedata as u\nout = u.normalize('NFC', '１')",
    "NFKC",
    "'１'",
    "'1'"
)]
#[case::composition(
    "unicode_normalization_form",
    "from unicodedata import normalize\nout = normalize('NFC', 'é')",
    "NFD",
    "'é'",
    "'e\\u0301'"
)]
#[case::remove(
    "unicode_normalization_remove",
    "from unicodedata import normalize as n\nout = n('NFKC', '１')",
    "",
    "'1'",
    "'１'"
)]
#[case::sharp_s("string_casefold_lower", "out = 'ß'.casefold()", "", "'ss'", "'ß'")]
#[case::prefix(
    "string_affix_strip",
    "out = 'ababaX'.removeprefix('ab')",
    "",
    "'abaX'",
    "'X'"
)]
#[case::replace(
    "encoding_errors_replace",
    "data = b'\\xff'\ntry: out = data.decode()\nexcept UnicodeDecodeError: out = None",
    "",
    "None",
    "'�'"
)]
#[case::ignore(
    "encoding_errors_ignore",
    "data = b'\\xff'\ntry: out = data.decode()\nexcept UnicodeDecodeError: out = None",
    "",
    "None",
    "''"
)]
#[case::strict(
    "encoding_errors_strict",
    "data = b'\\xff'\ntry: out = data.decode(errors='ignore')\nexcept UnicodeDecodeError: out = None",
    "",
    "''",
    "None"
)]
#[case::codec_order(
    "encoding_errors_replace",
    "import codecs as c\nevents = []\ndef see(x): events.append(x); return x\nout = c.decode(see(b'\\xff'), encoding=see('utf8'), errors=see('ignore')), events",
    "",
    "('', [b'\\xff', 'utf8', 'ignore'])",
    "('�', [b'\\xff', 'utf8', 'ignore'])"
)]
#[case::custom_receiver(
    "unicode_digit_ascii",
    "events = []\nclass Receiver:\n def isdigit(self): events.append('digit'); return True\n def isascii(self): events.append('ascii'); return False\ndef receiver(): events.append('receiver'); return Receiver()\nout = receiver().isdigit(), events",
    "",
    "(True, ['receiver','digit'])",
    "(False, ['receiver','digit','ascii'])"
)]
#[case::explicit_concatenation(
    "encoding_errors_replace",
    "data = b'\\xff'\nout = data.decode(errors='ig' 'nore')",
    "",
    "''",
    "'�'"
)]
#[case::format_policy(
    "encoding_errors_replace",
    "data = b'\\xff'\nchecks = []\ndef policy(): checks.append(1); return 'ignore'\nout = data.decode(errors=f'{policy()}'), checks",
    "",
    "('', [1])",
    "('�', [1])"
)]
#[case::bytes_policy(
    "encoding_errors_ignore",
    "data = b'\\xff'\ntry: out = data.decode(errors=b'ignore')\nexcept TypeError: out = None",
    "",
    "None",
    "''"
)]
#[case::positional_codec(
    "encoding_errors_replace",
    "import codecs\nevents = []\ndef see(x): events.append(x); return x\nout = codecs.decode(see(b'\\xff'), see('utf8'), see('ignore')), events",
    "",
    "('', [b'\\xff','utf8','ignore'])",
    "('�', [b'\\xff','utf8','ignore'])"
)]
#[case::normalization_effects(
    "unicode_normalization_remove",
    "from unicodedata import normalize as n\nevents = []\ndef see(x): events.append(x); return x\nout = n(see('NFKC'), (value := see('１'))), value, events",
    "",
    "('1','１',['NFKC','１'])",
    "('１','１',['NFKC','１'])"
)]
#[case::skips_ascii_after_false(
    "unicode_digit_ascii",
    "class Receiver:\n def isdecimal(self): return False\n def isascii(self): raise AssertionError\nout = Receiver().isdecimal()",
    "",
    "False",
    "False"
)]
#[case::codec_errors_before_encoding(
    "encoding_errors_replace",
    "import codecs as c\nevents = []\ndef see(x): events.append(x); return x\nout = c.decode(see(b'\\xff'), errors=see('ignore'), encoding=see('utf8')), events",
    "",
    "('', [b'\\xff','ignore','utf8'])",
    "('�', [b'\\xff','ignore','utf8'])"
)]
#[tokio::test]
async fn generated_candidate_changes_text_contract(
    #[case] id: &str,
    #[case] source: &str,
    #[case] destination: &str,
    #[case] original: &str,
    #[case] mutated: &str,
) {
    // Arrange
    assert_python(source, &format!("assert out == {original}, out")).await;

    // Act
    let script = mutant(source, id, destination);

    // Assert
    assert_python(&script, &format!("assert out == {mutated}, out")).await;
}

#[rstest::rstest]
#[case::decimal("isdecimal")]
#[case::digit("isdigit")]
#[case::numeric("isnumeric")]
#[tokio::test]
async fn ascii_narrowing_preserves_ascii_predicate(#[case] predicate: &str) {
    // Arrange
    let source = format!("def classify(s): return s.{predicate}()");
    let property = format!(
        "from itertools import product\nfor n in range(3):\n for chars in product(map(chr, range(128)), repeat=n):\n  text = ''.join(chars)\n  assert classify(text) == text.{predicate}(), repr(text)"
    );

    // Act
    let script = mutant(&source, "unicode_digit_ascii", "");

    // Assert
    assert_python(&script, &property).await;
}
