use super::{ContractCase, JSON_RUN_HARNESS, assert_contract};

#[rstest::rstest]
#[case::list_1("list", "a.items.append(3)", "lambda shared=[]: shared")]
#[case::dict_2("dict", "a.items['x']=3", "lambda shared={}: shared")]
#[case::set_3("set", "a.items.add(3)", "lambda shared={*()}: shared")]
#[tokio::test]
async fn dataclass_factory_sharing_changes_only_cross_instance_ownership(
    #[case] factory: &str,
    #[case] update: &str,
    #[case] replacement: &str,
) {
    let source = format!(
        "from dataclasses import dataclass,field\n@dataclass\nclass Bag:\n items: object = field(default_factory={factory}, init=False, repr=False, compare=False)\ndef run():\n a,b=Bag(),Bag()\n {update}\n return [len(a.items),len(b.items),a.items is b.items]\n"
    );
    assert_contract(ContractCase {
        name: factory,
        operator: "dataclass_factory_share",
        original: factory,
        replacement,
        source: &source,
        harness: JSON_RUN_HARNESS,
        baseline_stdout: "[1,0,false]\n",
        mutant_stdout: "[1,1,true]\n",
    })
    .await;
}

#[tokio::test]
async fn sort_key_swap_preserves_read_counts_and_changes_priority_and_order() {
    assert_contract(ContractCase {
        name:"sorting priority and evaluation order",operator:"sort_key_swap",
        original:"row[0],row[1]",replacement:"row[1],row[0]",
        source:concat!(
            "reads=[]\nclass Row:\n def __init__(self,label,values):\n  self.label=label\n  self.values=values\n",
            " def __getitem__(self,key):\n  reads.append([self.label,key])\n  return self.values[key]\n",
            "def run():\n rows=[Row('x',(2,1)),Row('y',(1,9)),Row('z',(1,3))]\n",
            " ordered=sorted(rows,key=lambda row:(row[0],row[1]))\n return [[row.label for row in ordered],reads]\n",
        ),harness:JSON_RUN_HARNESS,
        baseline_stdout:"[[\"z\",\"y\",\"x\"],[[\"x\",0],[\"x\",1],[\"y\",0],[\"y\",1],[\"z\",0],[\"z\",1]]]\n",
        mutant_stdout:"[[\"x\",\"z\",\"y\"],[[\"x\",1],[\"x\",0],[\"y\",1],[\"y\",0],[\"z\",1],[\"z\",0]]]\n",
    }).await;
}

#[rstest::rstest]
#[case::value_1("value")]
#[case::hoimin_get_2("_hoimin_get")]
#[case::hoimin_missing_3("_hoimin_missing")]
#[tokio::test]
async fn late_capture_keeps_explicit_arguments_and_helper_names_private(#[case] name: &str) {
    let mut getter = "_hoimin_get".to_owned();
    if getter == name {
        getter.push('_');
    }
    let mut missing = "_hoimin_missing".to_owned();
    if missing == name {
        missing.push('_');
    }
    let original = format!("lambda {name}={name}: {name}");
    let replacement = format!(
        "(lambda {getter}, {missing}: lambda {name}={missing}: ({getter}() if {name} is {missing} else {name}))(lambda: {name}, [])"
    );
    let source = format!(
        "def build(values):\n callbacks=[]\n for {name} in values:\n  callbacks.append({original})\n return callbacks\ndef run():\n callbacks=build([4,9])\n marker=[]\n return [[f() for f in callbacks],[f(4) for f in callbacks],[f({name}=None) for f in callbacks],all(f(marker) is marker for f in callbacks)]\n"
    );
    assert_contract(ContractCase {
        name,
        operator: "lambda_capture_late",
        original: &original,
        replacement: &replacement,
        source: &source,
        harness: JSON_RUN_HARNESS,
        baseline_stdout: "[[4,9],[4,4],[null,null],true]\n",
        mutant_stdout: "[[9,9],[4,4],[null,null],true]\n",
    })
    .await;
}

#[rstest::rstest]
#[case::def_run_data_b_a_xffb_return_data_decode_utf_8_errors_s_1(
    "def run():\n data=b'a\\xffb'\n return data.decode('utf-8', errors='surrogateescape')\n",
    "'surrogateescape'",
    "\"a\\udcffb\"\n",
    "\"ab\"\n"
)]
#[case::def_run_text_a_udcffb_return_list_text_encode_utf_8_err_2(
    "def run():\n text='a\\udcffb'\n return list(text.encode('utf-8', errors='surrogateescape'))\n",
    "'surrogateescape'",
    "[97,255,98]\n",
    "[97,98]\n"
)]
#[case::from_codecs_import_decode_as_dec_def_run_return_dec_b_a_3(
    "from codecs import decode as dec\ndef run():\n return dec(b'a\\xffb','utf-8',errors='surrogateescape')\n",
    "'surrogateescape'",
    "\"a\\udcffb\"\n",
    "\"ab\"\n"
)]
#[tokio::test]
async fn encoding_policy_loss_changes_data_without_runtime_errors(
    #[case] source: &str,
    #[case] original: &str,
    #[case] baseline: &str,
    #[case] mutant: &str,
) {
    assert_contract(ContractCase {
        name: "encoding policy",
        operator: "encoding_errors_ignore",
        original,
        replacement: "'ignore'",
        source,
        harness: JSON_RUN_HARNESS,
        baseline_stdout: baseline,
        mutant_stdout: mutant,
    })
    .await;
}

#[tokio::test]
async fn class_namespace_callbacks_stay_early_while_method_local_capture_is_late() {
    assert_contract(ContractCase {
        name:"class namespace is not a closure",operator:"lambda_capture_late",
        original:"lambda value=value: value",
        replacement:"(lambda _hoimin_get, _hoimin_missing: lambda value=_hoimin_missing: (_hoimin_get() if value is _hoimin_missing else value))(lambda: value, [])",
        source:concat!(
            "class Example:\n callbacks=[]\n for value in [4,9]:\n  callbacks.append(lambda value=value: value)\n",
            " def build(self):\n  callbacks=[]\n  for value in [1,7]:\n   callbacks.append(lambda value=value: value)\n  return callbacks\n",
            "def run(): return [[f() for f in Example.callbacks],[f() for f in Example().build()]]\n",
        ),harness:JSON_RUN_HARNESS,
        baseline_stdout:"[[4,9],[1,7]]\n",mutant_stdout:"[[4,9],[7,7]]\n",
    }).await;
}
