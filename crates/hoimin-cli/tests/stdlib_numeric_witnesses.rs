#[path = "common/stdlib_witness.rs"]
mod witness;
use witness::{assert_python, mutant};

#[rstest::rstest]
#[case::absolute(
    "math_isclose_tolerance",
    "from math import isclose",
    "isclose(0, 1e-10, abs_tol=(margin := 1e-9))",
    "[1]",
    "out is True",
    "out is False and margin == 1e-9"
)]
#[case::relative(
    "math_isclose_tolerance",
    "import math as m",
    "m.isclose(b=1+1e-10, a=1)",
    "",
    "out is True",
    "out is False"
)]
#[case::expm1(
    "math_precision_loss",
    "from math import expm1 as small",
    "2 * small(1e-16)",
    "",
    "out == 2e-16",
    "out == 0.0"
)]
#[case::log1p(
    "math_precision_loss",
    "import math as m",
    "m.log1p(1e-16)",
    "",
    "out == 1e-16",
    "out == 0.0"
)]
#[case::sample_variance(
    "statistics_sample_population",
    "from statistics import variance as spread",
    "spread([1,2,3])",
    "",
    "out == 1",
    "out == 2/3"
)]
#[case::population_variance(
    "statistics_sample_population",
    "import statistics as s",
    "s.pvariance([1,2,3], 2)",
    "",
    "out == 2/3",
    "out == 1"
)]
#[case::sample_stdev(
    "statistics_sample_population",
    "import statistics as s",
    "s.stdev([1,2,3], xbar=2)",
    "",
    "out == 1",
    "out == (2/3)**0.5"
)]
#[case::population_stdev(
    "statistics_sample_population",
    "from statistics import pstdev",
    "pstdev(mu=2, data=[1,2,3])",
    "",
    "out == (2/3)**0.5",
    "out == 1"
)]
#[tokio::test]
async fn numerical_boundary(
    #[case] id: &str,
    #[case] imports: &str,
    #[case] expr: &str,
    #[case] target: &str,
    #[case] original: &str,
    #[case] changed: &str,
) {
    // Arrange
    let source = format!("{imports}\nout = {expr}");
    assert_python(&source, &format!("assert {original},out")).await;
    // Act
    let mutated = mutant(&source, id, target);
    // Assert
    assert_python(&mutated, &format!("assert {changed},out")).await;
}

#[rstest::rstest]
#[case::pushpop("heappushpop", "[5,9]", "(3,[5,9])", "(5,[3,9])")]
#[case::replace("heapreplace", "[5,9]", "(5,[3,9])", "(3,[5,9])")]
#[case::empty_pushpop("heappushpop", "[]", "(3,[])", "(IndexError,[])")]
#[case::empty_replace("heapreplace", "[]", "(IndexError,[])", "(3,[])")]
#[tokio::test]
async fn heap_removal(
    #[case] api: &str,
    #[case] heap: &str,
    #[case] original: &str,
    #[case] changed: &str,
) {
    // Arrange
    let source = format!(
        "from heapq import {api} as take\nheap={heap}\ntry: out=take(heap,3)\nexcept IndexError: out=IndexError"
    );
    assert_python(&source, &format!("assert (out,heap)=={original}")).await;
    // Act
    let mutated = mutant(&source, "heap_pushpop_replace", "");
    // Assert
    assert_python(&mutated, &format!("assert (out,heap)=={changed}")).await;
}

#[rstest::rstest]
#[case::center_before_data(
    "from statistics import variance as v",
    "v(xbar=seen('center',2), data=seen('data',[1,2,3]))",
    "['center','data']",
    "1",
    "2/3"
)]
#[case::data_before_center(
    "from statistics import pvariance as v",
    "v(data=seen('data',[1,2,3]), mu=seen('center',2))",
    "['data','center']",
    "2/3",
    "1"
)]
#[tokio::test]
async fn keyword_center_evaluation(
    #[case] imports: &str,
    #[case] expression: &str,
    #[case] events: &str,
    #[case] original: &str,
    #[case] changed: &str,
) {
    // Arrange
    let source = format!(
        "{imports}\nevents=[]\ndef seen(name,value): events.append(name); return value\nout={expression}"
    );
    assert_python(
        &source,
        &format!("assert out=={original} and events=={events}"),
    )
    .await;
    // Act
    let mutated = mutant(&source, "statistics_sample_population", "");
    // Assert
    assert_python(
        &mutated,
        &format!("assert out=={changed} and events=={events}"),
    )
    .await;
}

#[tokio::test]
async fn explicit_relative_tolerance_preserves_keyword_order_and_binding() {
    // Arrange
    let source = "from math import isclose\nevents=[]\ndef seen(name,v): events.append(name); return v\nout=isclose(rel_tol=(margin:=seen('rel',1e-3)),b=seen('b',1.0001),abs_tol=0,a=seen('a',1))";
    let effects = "margin==1e-3 and events==['rel','b','a']";
    assert_python(source, &format!("assert out and {effects}")).await;
    // Act
    let mutated = mutant(source, "math_isclose_tolerance", "");
    // Assert
    assert_python(&mutated, &format!("assert not out and {effects}")).await;
}

#[tokio::test]
async fn precision_keeps_awaited_argument_in_async_function() {
    // Arrange
    let source = "import asyncio\nfrom math import log1p as tiny\nevents=[]\nasync def fetch(): events.append(1); return 1e-16\nasync def measure(): return tiny(await fetch())\nout=asyncio.run(measure())";
    assert_python(source, "assert out==1e-16 and events==[1]").await;
    // Act
    let mutated = mutant(source, "math_precision_loss", "");
    // Assert
    assert_python(&mutated, "assert out==0 and events==[1]").await;
}

#[tokio::test]
async fn heap_preserves_receiver_and_item_evaluation() {
    // Arrange
    let source = "import heapq as h\nevents=[]\nheap=[5,9]\ndef mark(name,obj): events.append(name);return obj\nout=h.heappushpop(mark('heap',heap),mark('item',3))";
    assert_python(
        source,
        "assert (out,heap,events)==(3,[5,9],['heap','item'])",
    )
    .await;
    // Act
    let mutated = mutant(source, "heap_pushpop_replace", "");
    // Assert
    assert_python(
        &mutated,
        "assert (out,heap,events)==(5,[3,9],['heap','item'])",
    )
    .await;
}

#[tokio::test]
async fn zero_tolerance_agrees_with_finite_equality() {
    // Arrange
    let source = "from math import isclose\ndef close(a,b): return isclose(a,b)";
    let values = r"
from math import nextafter, inf
numbers = [0., -0., 5e-324, -5e-324, 1e-308, 1e-16, -1., 1., 1e308]
values = numbers + [nextafter(x, direction) for x in numbers for direction in (-inf,inf)]
";
    // Act
    let mutated = mutant(source, "math_isclose_tolerance", "");
    // Assert
    assert_python(
        &mutated,
        &format!("{values}\nfor a in values:\n for b in values: assert close(a,b) == (a==b),(a,b)"),
    )
    .await;
}

#[tokio::test]
async fn population_variance_has_the_population_denominator() {
    // Arrange
    let source = "import statistics as s\ndef spread(xs): return s.variance(xs)";
    let samples = r"
from fractions import Fraction as F
from itertools import product
for n in range(2,6):
 for xs in product(map(F, [-2,0,3]), repeat=n):
  center = sum(xs) / n
  sample = sum((x-center)**2 for x in xs) / (n-1)
";
    assert_python(source, &format!("{samples}  assert spread(xs)==sample")).await;
    // Act
    let mutated = mutant(source, "statistics_sample_population", "");
    // Assert
    assert_python(
        &mutated,
        &format!("{samples}  assert n*spread(xs)==(n-1)*sample,(n,xs)"),
    )
    .await;
}

#[rstest::rstest]
#[case::pushpop("heappushpop")]
#[case::replace("heapreplace")]
#[tokio::test]
async fn heap_conserves_inserted_and_removed_values(#[case] api: &str) {
    // Arrange
    let source = format!("from heapq import {api}\ndef step(heap,item): return {api}(heap,item)");
    let property = r"
from heapq import heapify
from itertools import product
for n in range(1,5):
 for xs in product([-1,0,2], repeat=n):
  for item in [-2,-1,0,2,3]:
   heap = list(xs)
   heapify(heap)
   out = step(heap,item)
   assert len(heap)==n and sorted([*heap,out])==sorted([*xs,item]),(xs,item)
   assert all(heap[(i-1)//2]<=heap[i] for i in range(1,n)),heap
";
    assert_python(&source, property).await;
    // Act
    let mutated = mutant(&source, "heap_pushpop_replace", "");
    // Assert
    assert_python(&mutated, property).await;
}

#[rstest::rstest]
#[case::member_keyword("from math import exp as ｉｆ, expm1 as tiny")]
#[case::module_keyword("import math as ｉｆ\nfrom math import expm1 as tiny")]
#[case::member_safe_alternative(
    "from math import exp as ｉｆ\nfrom math import exp as other, expm1 as tiny\n__import__=None"
)]
#[case::module_safe_alternative(
    "import math as ｉｆ\nimport math as other\nfrom math import expm1 as tiny\n__import__=None"
)]
#[case::normalized_identifier("from math import exp as ｅｘｐ, expm1 as tiny\n__import__=None")]
#[case::soft_keyword("from math import exp as ｍａｔｃｈ, expm1 as tiny\n__import__=None")]
#[tokio::test]
async fn normalized_destination_runs(#[case] imports: &str) {
    // Arrange
    let source = format!("{imports}\nout=tiny(1e-16)");
    assert_python(&source, "assert out == 1e-16").await;
    // Act
    let mutated = mutant(&source, "math_precision_loss", "");
    // Assert
    assert_python(&mutated, "assert out == 0").await;
}

#[tokio::test]
async fn normalized_destination_in_string_returning_resolver() {
    // Arrange
    // Shadow round so the only available sibling is ceil, independently of
    // the source spelling selected to call it.
    let source = "from math import floor as original, ceil as ｉｆ\nround=None\nout=original(1.25)";
    assert_python(source, "assert out == 1").await;
    // Act
    let mutated = mutant(source, "rounding_function_swap", "");
    // Assert
    assert_python(&mutated, "assert out == 2").await;
}
