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
