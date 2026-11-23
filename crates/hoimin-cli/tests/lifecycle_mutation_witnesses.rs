use camino::Utf8Path;
use hoimin_core::MutationProfile;
use hoimin_python::{AnalyzeRequest, analyze_source_cancellable, payload::PayloadBudget};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn python() -> PathBuf {
    std::env::var_os("HOIMIN_OPERATOR_TEST_PYTHON").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
                "../../.venv/Scripts/python.exe"
            } else {
                "../../.venv/bin/python"
            })
        },
        PathBuf::from,
    )
}

async fn observe(source: &str, assertion: &str) {
    let script = format!("{source}\n{assertion}");
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(python())
            .args(["-B", "-c", &script])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{script}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[rstest::rstest]
#[case::sync_exit(
    "context_exit_omit",
    "events=[]\nclass CM:\n def __enter__(self): events.append('enter'); return 7\n def __exit__(self,*exc): events.append('exit')\nwith CM() as value: events.append(value)\n",
    "assert events == ['enter',7,'exit']",
    "assert events == ['enter',7]"
)]
#[case::async_exit(
    "context_exit_omit",
    "events=[]\nclass CM:\n async def __aenter__(self): events.append('enter'); return 7\n async def __aexit__(self,*exc): events.append('exit')\nasync def f():\n async with CM() as value: events.append(value)\ntry: f().send(None)\nexcept StopIteration: pass\n",
    "assert events == ['enter',7,'exit']",
    "assert events == ['enter',7]"
)]
#[case::async_cancelled_exit_unwinds_other_manager(
    "context_exit_omit",
    r"import asyncio
events=[]
class CM:
 def __init__(self,name): self.name=name
 async def __aenter__(self): events.append(('enter',self.name))
 async def __aexit__(self,kind,*exc): events.append(('exit',self.name,kind.__name__))
async def worker(started):
 async with CM('selected'), CM('neighbor'):
  events.append('body')
  started.set()
  await asyncio.Future()
async def main():
 started=asyncio.Event()
 task=asyncio.create_task(worker(started))
 await started.wait()
 task.cancel()
 try: await task
 except asyncio.CancelledError: events.append('cancelled')
asyncio.run(main())
",
    "assert events == [('enter','selected'),('enter','neighbor'),'body',('exit','neighbor','CancelledError'),('exit','selected','CancelledError'),'cancelled'], events",
    "assert events == [('enter','selected'),('enter','neighbor'),'body',('exit','neighbor','CancelledError'),'cancelled'], events"
)]
#[case::sync_descriptor_binding(
    "context_exit_omit",
    "events=[]\nclass Descriptor:\n def __init__(self,name): self.name=name\n def __get__(self,obj,cls):\n  events.append(('bind',self.name,obj.label))\n  return lambda *args: events.append(self.name)\nclass CM:\n label='manager'\n __enter__=Descriptor('enter')\n __exit__=Descriptor('exit')\nwith CM(): events.append('body')\n",
    "assert events == [('bind','exit','manager'),('bind','enter','manager'),'enter','body','exit'], events",
    "assert events == [('bind','exit','manager'),('bind','enter','manager'),'enter','body'], events"
)]
#[case::async_descriptor_binding(
    "context_exit_omit",
    "events=[]\nclass Descriptor:\n def __init__(self,name): self.name=name\n def __get__(self,obj,cls):\n  events.append(('bind',self.name,obj.label))\n  async def call(*args): events.append(self.name)\n  return call\nclass CM:\n label='manager'\n __aenter__=Descriptor('enter')\n __aexit__=Descriptor('exit')\nasync def run():\n async with CM(): events.append('body')\ntry: run().send(None)\nexcept StopIteration: pass\n",
    "assert events == [('bind','exit','manager'),('bind','enter','manager'),'enter','body','exit'], events",
    "assert events == [('bind','exit','manager'),('bind','enter','manager'),'enter','body'], events"
)]
#[case::special_lookup_ignores_interception(
    "context_exit_omit",
    r"events=[]
class DescriptorMeta(type):
 def __getattribute__(self,name):
  raise AssertionError(('descriptor metaclass lookup',name))
class Enter(metaclass=DescriptorMeta):
 def __getattribute__(self,name):
  raise AssertionError(('descriptor instance lookup',name))
 def __get__(self,obj,cls): return lambda: obj.label
class Meta(type):
 def __getattribute__(self,name):
  raise AssertionError(('manager metaclass lookup',name))
class Base:
 __enter__=Enter()
 def __exit__(self,*exc): events.append('exit')
class CM(Base,metaclass=Meta):
 def __getattribute__(self,name):
  if name == 'label': return 'manager'
  raise AssertionError(('manager instance lookup',name))
with CM() as value: events.append(value)
",
    "assert events == ['manager','exit'], events",
    "assert events == ['manager'], events"
)]
#[case::sync_manager_lifetime(
    "context_exit_omit",
    "events=[]\nclass CM:\n def __enter__(self): events.append('enter')\n def __exit__(self,*exc): events.append('exit')\n def __del__(self): events.append('del')\nwith CM(): events.append('body')\nevents.append('after')\n",
    "assert events == ['enter','body','exit','del','after']",
    "assert events == ['enter','body','del','after']"
)]
#[case::async_manager_lifetime(
    "context_exit_omit",
    "events=[]\nclass CM:\n async def __aenter__(self): events.append('enter')\n async def __aexit__(self,*exc): events.append('exit')\n def __del__(self): events.append('del')\nasync def run():\n async with CM(): events.append('body')\n events.append('after')\ntry: run().send(None)\nexcept StopIteration: pass\n",
    "assert events == ['enter','body','exit','del','after']",
    "assert events == ['enter','body','del','after']"
)]
#[case::manager_lifetime_without_bound_reference(
    "context_exit_omit",
    "events=[]\nclass CM:\n @staticmethod\n def __enter__(): events.append('enter')\n @staticmethod\n def __exit__(*exc): events.append('exit')\n def __del__(self): events.append('del')\nwith CM(): events.append('body')\n",
    "assert events == ['del','enter','body','exit'], events",
    "assert events == ['del','enter','body'], events"
)]
#[case::missing_exit_prevents_entry_lookup(
    "context_exit_omit",
    r"events=[]
class Enter:
 def __get__(self,obj,cls):
  events.append('bind')
  return lambda: None
class CM:
 __enter__=Enter()
try:
 with CM(): events.append('body')
except TypeError as exc:
 events.append(str(exc))
",
    "assert events == [\"'CM' object does not support the context manager protocol (missed __exit__ method)\"], events",
    "assert events == [\"'CM' object does not support the context manager protocol (missed __exit__ method)\"], events"
)]
#[case::failed_exit_binding_does_not_keep_manager_alive(
    "context_exit_omit",
    r"events=[]
class CM:
 __exit__=property()
 def __enter__(self): events.append('enter')
 def __del__(self): events.append('del')
try:
 with CM(): events.append('body')
except AttributeError as exc:
 saved=exc
 events.append('caught')
",
    "assert events == ['del','caught'] and isinstance(saved, AttributeError), events",
    "assert events == ['del','caught'] and isinstance(saved, AttributeError), events"
)]
#[case::assignment_failure(
    "context_exit_omit",
    "events=[]\nclass CM:\n def __enter__(self): events.append('enter'); return []\n def __exit__(self,*exc): events.append(exc[0].__name__)\ntry:\n with CM() as (first,second): events.append('body')\nexcept ValueError: events.append('caught')\n",
    "assert events == ['enter','ValueError','caught']",
    "assert events == ['enter','caught']"
)]
#[case::failed_entry(
    "context_exit_omit",
    "events=[]\nclass CM:\n def __enter__(self): events.append('enter'); raise ValueError\n def __exit__(self,*exc): events.append('exit')\ntry:\n with CM(): events.append('body')\nexcept ValueError: events.append('caught')\n",
    "assert events == ['enter','caught']",
    "assert events == ['enter','caught']"
)]
#[case::multiple_managers(
    "context_exit_omit",
    "events=[]\nclass CM:\n def __init__(self,n): self.n=n\n def __enter__(self): events.append('enter'+self.n)\n def __exit__(self,*exc): events.append('exit'+self.n)\nwith CM('a'), CM('b'): events.append('body')\n",
    "assert events == ['entera','enterb','body','exitb','exita']",
    "assert events == ['entera','enterb','body','exitb']"
)]
#[case::exit_exception(
    "context_exit_omit",
    "events=[]\nclass CM:\n def __enter__(self): return self\n def __exit__(self,*exc): raise ValueError\ntry:\n with CM(): events.append('body')\nexcept ValueError: events.append('caught')\n",
    "assert events == ['body','caught']",
    "assert events == ['body']"
)]
#[case::suppression_after_cleanup(
    "context_exit_result_negate",
    "events=[]\nclass CM:\n def __enter__(self): return self\n def __exit__(self,*exc): events.append('cleanup'); return False\ntry:\n with CM(): raise ValueError\nexcept ValueError: events.append('caught')\n",
    "assert events == ['cleanup','caught']",
    "assert events == ['cleanup']"
)]
#[case::restore_propagation(
    "context_exit_result_negate",
    "events=[]\nclass CM:\n def __enter__(self): return self\n def __exit__(self,*exc): events.append('cleanup'); return True\ntry:\n with CM(): raise ValueError\nexcept ValueError: events.append('caught')\n",
    "assert events == ['cleanup']",
    "assert events == ['cleanup','caught']"
)]
#[case::async_suppression(
    "context_exit_result_negate",
    "events=[]\nclass CM:\n async def __aenter__(self): return self\n async def __aexit__(self,*exc): events.append('cleanup'); return False\nasync def f():\n try:\n  async with CM(): raise ValueError\n except ValueError: events.append('caught')\ntry: f().send(None)\nexcept StopIteration: pass\n",
    "assert events == ['cleanup','caught']",
    "assert events == ['cleanup']"
)]
#[case::bare_suppression_return(
    "context_exit_result_negate",
    "events=[]\nclass CM:\n def __enter__(self): return self\n def __exit__(self,*exc): events.append('cleanup'); return\ntry:\n with CM(): raise ValueError\nexcept ValueError: events.append('caught')\n",
    "assert events == ['cleanup','caught']",
    "assert events == ['cleanup']"
)]
#[case::implicit_suppression_return(
    "context_exit_result_negate",
    "events=[]\nclass CM:\n def __enter__(self): return self\n def __exit__(self,*exc): events.append('cleanup')\ntry:\n with CM(): raise ValueError\nexcept ValueError: events.append('caught')\n",
    "assert events == ['cleanup','caught']",
    "assert events == ['cleanup']"
)]
#[case::yield_effects(
    "yield_statement_remove",
    "events=[]\ndef value(): events.append('value'); return 1\ndef f():\n yield value()\n yield 2\nout=list(f())\n",
    "assert out == [1,2] and events == ['value']",
    "assert out == [2] and events == ['value']"
)]
#[case::last_yield_keeps_generator(
    "yield_statement_remove",
    "import inspect\nevents=[]\ndef f():\n yield 1\n events.append('tail')\nout=list(f())\n",
    "assert inspect.isgeneratorfunction(f) and out == [1] and events == ['tail']",
    "assert inspect.isgeneratorfunction(f) and out == [] and events == ['tail']"
)]
#[case::yield_close_cleanup(
    "yield_statement_remove",
    "events=[]\ndef f():\n try:\n  yield 1\n  yield 2\n finally: events.append('closed')\ng=f(); out=next(g); g.close()\n",
    "assert out == 1 and events == ['closed']",
    "assert out == 2 and events == ['closed']"
)]
#[case::yield_send_retains_next_point(
    "yield_statement_remove",
    "events=[]\ndef f():\n yield 1\n value = yield 2\n events.append(value)\ng=f(); first=next(g)\ntry: second=g.send(9)\nexcept StopIteration: second='done'\n",
    "assert first == 1 and second == 2 and events == []",
    "assert first == 2 and second == 'done' and events == [9]"
)]
#[case::yield_throw_retains_cleanup(
    "yield_statement_remove",
    "events=[]\ndef f():\n try:\n  yield 1\n  yield 2\n except ValueError: events.append('caught')\n finally: events.append('closed')\ng=f(); first=next(g)\ntry: g.throw(ValueError)\nexcept StopIteration: pass\n",
    "assert first == 1 and events == ['caught','closed']",
    "assert first == 2 and events == ['caught','closed']"
)]
#[case::async_generator_value_await(
    "yield_statement_remove",
    "import asyncio\nevents=[]\nasync def value(): events.append('value'); return 1\nasync def gen():\n yield await value()\n yield 2\nasync def collect(): return [x async for x in gen()]\nout=asyncio.run(collect())\n",
    "assert out == [1,2] and events == ['value']",
    "assert out == [2] and events == ['value']"
)]
#[case::implicit_return_after_multiline_suite(
    "context_exit_result_negate",
    "events=[]\nclass CM:\n def __enter__(self): return self\n def __exit__(self,*exc):\n  try: events.append('cleanup')\n  finally: events.append('finally') # tail\ntry:\n with CM(): raise ValueError\nexcept ValueError: events.append('caught')\n",
    "assert events == ['cleanup','finally','caught']",
    "assert events == ['cleanup','finally']"
)]
#[case::implicit_return_after_continued_inline_suite(
    "context_exit_result_negate",
    r"events=[]
class CM:
 def __enter__(self): pass
 def __exit__(self,*exc): \
  events.append('cleanup') # tail
try:
 with CM(): raise ValueError
except ValueError: events.append('caught')
",
    "assert events == ['cleanup','caught'], events",
    "assert events == ['cleanup'], events"
)]
#[case::implicit_return_uses_logical_block_indent(
    "context_exit_result_negate",
    r"events=[]
class CM:
 def __enter__(self): pass
 def __exit__(self,*exc):
  \
      events.append('cleanup') # tail
try:
 with CM(): raise ValueError
except ValueError: events.append('caught')
",
    "assert events == ['cleanup','caught'], events",
    "assert events == ['cleanup'], events"
)]
#[case::manager_walrus_original_scope(
    "context_exit_omit",
    "events=[]\nclass CM:\n def __enter__(self): return self\n def __exit__(self,*exc): events.append('exit')\ndef f():\n with (manager := CM()) as result: return result is manager\nresult=f()\n",
    "assert result and events == ['exit']",
    "assert result and events == []"
)]
#[case::shadowed_builtin_uses_proven_alias(
    "context_exit_omit",
    "from builtins import property as context_property\nproperty=None\nevents=[]\nclass CM:\n def __enter__(self): events.append('enter')\n def __exit__(self,*exc): events.append('exit')\nwith CM(): events.append('body')\n",
    "assert events == ['enter','body','exit']",
    "assert events == ['enter','body']"
)]
#[case::shadowed_import_is_unrelated(
    "context_exit_omit",
    "__import__=None\nevents=[]\nclass CM:\n def __enter__(self): events.append('enter')\n def __exit__(self,*exc): events.append('exit')\nwith CM(): events.append('body')\n",
    "assert events == ['enter','body','exit']",
    "assert events == ['enter','body']"
)]
#[tokio::test]
async fn lifecycle_mutation_has_the_named_effect(
    #[case] id: &str,
    #[case] source: &str,
    #[case] original: &str,
    #[case] expected: &str,
) {
    observe_mutation(id, source, None, original, expected).await;
}

#[rstest::rstest]
#[case::exceptional_exit_after_finally(
    "decision",
    r"events=[]
class Decision:
 ready=False
 def __bool__(self):
  events.append(('truth',self.ready))
  return self.ready
class CM:
 def __enter__(self): pass
 def __exit__(self,*exc):
  decision=Decision()
  try: return decision
  finally:
   events.append('cleanup')
   decision.ready=True
try:
 with CM(): raise ValueError
except ValueError: events.append('caught')
",
    "assert events == ['cleanup',('truth',True)], events",
    "assert events == ['cleanup',('truth',True),'caught'], events"
)]
#[case::normal_exit(
    "Decision()",
    r"events=[]
class Decision:
 def __bool__(self): raise AssertionError('normal exit tested suppression')
class CM:
 def __enter__(self): pass
 def __exit__(self,*exc):
  try: return Decision()
  finally: events.append('cleanup')
with CM(): events.append('body')
",
    "assert events == ['body','cleanup'], events",
    "assert events == ['body','cleanup'], events"
)]
#[case::retained_truth_failure_releases_decision(
    "Decision()",
    r"events=[]
class Decision(list):
 __len__=None
 def __del__(self): events.append('del')
class CM:
 def __enter__(self): pass
 def __exit__(self,*exc): return Decision()
try:
 with CM(): raise ValueError
except TypeError as exc:
 saved=exc
 events.append('caught')
",
    "assert events == ['del','caught'] and isinstance(saved, TypeError), events",
    "assert events == ['del','caught'] and isinstance(saved, TypeError), events"
)]
#[tokio::test]
async fn exit_decision_truth_is_tested_only_by_the_context_protocol(
    #[case] decision: &str,
    #[case] source: &str,
    #[case] original: &str,
    #[case] expected: &str,
) {
    observe_mutation(
        "context_exit_result_negate",
        source,
        Some(decision),
        original,
        expected,
    )
    .await;
}

async fn observe_mutation(
    id: &str,
    source: &str,
    selected_original: Option<&str>,
    original_assertion: &str,
    mutant_assertion: &str,
) {
    // Arrange
    let operators = serde_json::from_value(serde_json::json!([id])).unwrap();
    let request = AnalyzeRequest {
        payload: PayloadBudget::default(),
        path: Utf8Path::new("subject.py"),
        lines: &[],
        symbols: &[],
        operators: &operators,
        profile: MutationProfile::Full,
        max_candidates: 100,
    };
    observe(source, original_assertion).await;

    // Act
    let output = analyze_source_cancellable(&request, source, || false).unwrap();
    let candidate = output
        .candidates
        .iter()
        .find(|candidate| selected_original.is_none_or(|text| candidate.original == text))
        .expect("lifecycle mutation of the selected source");
    let mut mutant = source.to_owned();
    let start = usize::try_from(candidate.span.start).unwrap();
    let end = start + usize::try_from(candidate.span.length).unwrap();
    mutant.replace_range(start..end, &candidate.replacement);

    // Assert
    observe(&mutant, mutant_assertion).await;
}
