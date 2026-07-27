from dataclasses import dataclass
import math
import re


_DURATION = re.compile(r"(?P<value>(?:\d+(?:\.\d*)?|\.\d+))(?P<unit>[smh])")
_SECONDS_PER_UNIT = {"s": 1.0, "m": 60.0, "h": 3_600.0}


def parse_duration(text: str) -> float:
    match = _DURATION.fullmatch(text)
    if match is None:
        raise ValueError(f"invalid duration: {text!r}")
    seconds = float(match.group("value")) * _SECONDS_PER_UNIT[match.group("unit")]
    if not math.isfinite(seconds) or seconds <= 0.0:
        raise ValueError("duration must be positive and finite")
    return seconds


@dataclass(frozen=True)
class RunBudget:
    started: float
    discovery_deadline: float
    mutation_deadline: float
    deadline: float

    @classmethod
    def start(cls, total_seconds: float, now: float) -> "RunBudget":
        if not math.isfinite(total_seconds) or total_seconds <= 0.0:
            raise ValueError("total budget must be positive and finite")
        report = 300.0 if total_seconds >= 900.0 else total_seconds / 6.0
        discovery = min(600.0, total_seconds / 3.0)
        return cls(now, now + discovery, now + total_seconds - report, now + total_seconds)

    def discovery_timeout(self, now: float) -> float:
        return max(0.0, self.discovery_deadline - now)

    def mutation_timeout(self, now: float) -> float:
        return max(0.0, self.mutation_deadline - now)

    def may_start_mutation(self, now: float) -> bool:
        return now < self.mutation_deadline
