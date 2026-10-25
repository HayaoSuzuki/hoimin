use super::mutation_observations::assert_change;

#[tokio::test]
async fn effective_python_none_truthiness_checks_empty_values_and_bool_results() {
    assert_change("def missing(value): return value is None\n", "none_truthiness", "values=[None,False,0,'',[],1]\nresults=[missing(x) for x in values]\nprint(results); print(all(type(x) is bool for x in results))", "[True, False, False, False, False, False]\nTrue\n", "[True, True, True, True, True, False]\nTrue\n").await;
    assert_change("def supplied(value): return None is not value\n", "none_truthiness", "values=[None,False,0,'',[],1]\nresults=[supplied(x) for x in values]\nprint(results); print(all(type(x) is bool for x in results))", "[False, True, True, True, True, True]\nTrue\n", "[False, False, False, False, False, True]\nTrue\n").await;
    assert_change("def missing(next, bool, values): return next(values) is None\n", "none_truthiness", "events=[]\ndef get(values): events.append('once'); return 0\nprint(missing(get, None, [])); print(events)", "False\n['once']\n", "True\n['once']\n").await;
}

#[tokio::test]
async fn effective_python_shared_defaults_keep_explicit_none_fresh() {
    assert_change("def make(*, items=None):\n    'doc'\n    if items is None: items=[]\n    return items\n", "mutable_default_share", "a=make(); a.append(1)\nprint(make()); print(make(items=None))\nsupplied=[]; print(make(items=supplied) is supplied); print(make.__doc__)", "[]\n[]\nTrue\ndoc\n", "[1]\n[]\nTrue\ndoc\n").await;
    assert_change(
        "def make(items=None, /):\n    if None is items: items={}\n    return items\n",
        "mutable_default_share",
        "a=make(); a['x']=1\nprint(make()); print(make(None))",
        "{}\n{}\n",
        "{'x': 1}\n{}\n",
    )
    .await;
    assert_change("class C:\n    def __init__(self, items=None):\n        if items is None: items=[]\n        self.items=items\n", "mutable_default_share", "a=C(); a.items.append(1)\nprint(C().items); print(C(None).items)", "[]\n[]\n", "[1]\n[]\n").await;
}

#[tokio::test]
async fn effective_python_nonlocal_checks_outer_state_and_inline_suites() {
    assert_change("def outer():\n    first=second=0\n    def inner(): nonlocal first, second; first=second=1\n    inner()\n    return first, second\n", "nonlocal_remove", "print(outer())", "(1, 1)\n", "(0, 0)\n").await;
}

#[tokio::test]
async fn effective_python_yield_delegation_checks_value_shape_and_comments() {
    assert_change("def values():\n    yield from (\n        # preserve operand comment\n        (1, 2)\n    )\n", "yield_from_to_yield", "print(list(values()))", "[1, 2]\n", "[(1, 2)]\n").await;
    assert_change(
        "def values():\n    yield \\\n        from \\\n        (1, 2)\n",
        "yield_from_to_yield",
        "print(list(values()))",
        "[1, 2]\n",
        "[(1, 2)]\n",
    )
    .await;
}

#[tokio::test]
async fn effective_python_setdefault_checks_saved_state_and_argument_effects_once() {
    assert_change("events=[]\nd={}\ndef key():\n    events.append('key')\n    return 'x'\ndef default():\n    events.append('default')\n    return []\nvalue=d.setdefault(key(), default())\n", "mapping_setdefault_to_get", "value.append(1)\nprint(value); print(d); print(events)", "[1]\n{'x': [1]}\n['key', 'default']\n", "[1]\n{}\n['key', 'default']\n").await;
    assert_change(
        "d={}\nvalue=d.setdefault('x')\n",
        "mapping_setdefault_to_get",
        "print(value); print(d)",
        "None\n{'x': None}\n",
        "None\n{}\n",
    )
    .await;
}

#[tokio::test]
async fn effective_python_parenthesized_none_keeps_values_and_boolean_results() {
    for source in [
        "def missing(value): return value is (None)\n",
        "def missing(value): return ((None)) is value\n",
        "def missing(value): return (value is (\n    None # keep\n))\n",
    ] {
        assert_change(
            source,
            "none_truthiness",
            "print(missing(0)); print(missing(None))",
            "False\nTrue\n",
            "True\nTrue\n",
        )
        .await;
    }
    for source in [
        "def supplied(value): return value is not ((None))\n",
        "def supplied(value): return ((None)) is not value\n",
    ] {
        assert_change(
            source,
            "none_truthiness",
            "print(supplied(0)); print(supplied(None)); print(type(supplied(1)).__name__)",
            "True\nFalse\nbool\n",
            "False\nFalse\nbool\n",
        )
        .await;
    }
}

#[tokio::test]
async fn effective_python_extra_boundary_copy_and_fifo_contracts() {
    assert_change(
        "import bisect\ndef boundary(xs, x): return bisect.bisect_left(xs, x)\n",
        "bisect_boundary_swap",
        "print(boundary([1,1,2], 1)); print(boundary([], 1)); print(boundary([1,2], 3))",
        "0\n0\n2\n",
        "2\n0\n2\n",
    )
    .await;
    assert_change(
        "from bisect import bisect_right as boundary\ndef index(xs, x): return boundary(xs, x)\n",
        "bisect_boundary_swap",
        "print(index([1,1,2], 1))",
        "2\n",
        "0\n",
    )
    .await;
    assert_change("from copy import deepcopy as clone\nevents=[]\ndef original():\n    events.append('once')\n    return source\nsource=[[1]]\nresult=clone(original())\n", "deepcopy_to_copy", "result[0].append(2)\nprint(source); print(result); print(events)", "[[1]]\n[[1, 2]]\n['once']\n", "[[1, 2]]\n[[1, 2]]\n['once']\n").await;
    assert_change(
        "from collections import deque\nq=deque([1, 2])\nvalue=q.popleft()\n",
        "deque_popleft_to_pop",
        "print(value); print(list(q))",
        "1\n[2]\n",
        "2\n[1]\n",
    )
    .await;
}

#[tokio::test]
async fn effective_python_gather_checks_failure_propagation_and_success() {
    assert_change("import asyncio\nevents=[]\nasync def failure(): raise ValueError('bad')\ndef make():\n    events.append('made')\n    return failure()\nasync def check():\n    try:\n        result=await asyncio.gather(\n            make(), # keep\n        )\n    except ValueError:\n        print('raised')\n    else:\n        print([type(x).__name__ for x in result])\n    print(events)\n", "gather_return_exceptions", "asyncio.run(check())", "raised\n['made']\n", "['ValueError']\n['made']\n").await;
    assert_change("import asyncio\nfrom asyncio import gather as collect\nasync def success(): return 7\nasync def check(): print(await collect(success()))\n", "gather_return_exceptions", "asyncio.run(check())", "[7]\n", "[7]\n").await;
}
#[tokio::test]
async fn effective_python_iterator_prefix_checks_values_and_consumption() {
    assert_change("import itertools\nevents=[]\ndef predicate(x):\n    events.append(x)\n    return x<3\nxs=iter([1,2,3,1])\nresult=itertools.takewhile(predicate,xs)\n", "itertools_take_drop_swap", "print(list(result)); print(list(xs)); print(events)", "[1, 2]\n[1]\n[1, 2, 3]\n", "[3, 1]\n[]\n[1, 2, 3]\n").await;
    assert_change("from itertools import dropwhile as remaining\ndef select(xs): return list(remaining(lambda x:x<3,xs))\n", "itertools_take_drop_swap", "print(select([1,2,3,1])); print(select([]))", "[3, 1]\n[]\n", "[1, 2]\n[]\n").await;
}
#[tokio::test]
async fn effective_python_fstring_checks_representation_and_effects_once() {
    assert_change("events=[]\nclass Label:\n    def __repr__(self):\n        events.append('repr')\n        return 'debug'\n    def __str__(self):\n        events.append('str')\n        return 'display'\ndef make():\n    events.append('make')\n    return Label()\nresult=f'{make()!r:>8}'\n", "fstring_conversion_swap", "print(result); print(events)", "   debug\n['make', 'repr']\n", " display\n['make', 'str']\n").await;
    assert_change("class Width:\n    def __str__(self): return '4'\n    def __repr__(self): return '6'\nwidth=Width()\nresult=f'{12:{width!s}}'\n", "fstring_conversion_swap", "print(repr(result))", "'  12'\n", "'    12'\n").await;
    assert_change("class Label:\n    def __str__(self): return 'display'\n    def __repr__(self): return 'debug'\nvalue=Label()\nresult=f'{value=!s}'\n", "fstring_conversion_swap", "print(result)", "value=display\n", "value=debug\n").await;
}
