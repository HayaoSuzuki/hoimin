import HoiminOracle.Model

namespace HoiminOracle

example : (step State.initial (.complete 99 .ordinary)).errorCode? =
    some "machine.effect.unknown" := by decide

example :
    let state := State.withPending 1 .ordinary
    (step state (.complete 1 .cleanup)).errorCode? =
      some "machine.effect.wrong_completion" := by decide

example :
    let state := State.withPending 1 .ordinary
    let accepted := (step state (.complete 1 .ordinary)).state
    (step accepted (.complete 1 .ordinary)).errorCode? =
      some "machine.effect.duplicate" := by decide

example :
    let state := State.withPending 1 .ordinary
    let stopped := (step state (.stop .cancelled)).state
    (step stopped (.complete 1 .ordinary)).errorCode? =
      some "machine.effect.retired" := by decide

end HoiminOracle
