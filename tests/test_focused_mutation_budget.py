from pathlib import Path
import json
import tempfile
import unittest

from tools.focused_mutation_support.budget import RunBudget, parse_duration
from tools.focused_mutation_support.model import RunRecord, RunState
from tools.focused_mutation_support.store import RunStore


class BudgetTests(unittest.TestCase):
    def test_duration_accepts_positive_seconds_minutes_and_hours(self) -> None:
        self.assertEqual(parse_duration("90s"), 90.0)
        self.assertEqual(parse_duration("30m"), 1_800.0)
        self.assertEqual(parse_duration("1.5h"), 5_400.0)
        for value in ("0m", "-1s", "30", "nanm"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                parse_duration(value)

    def test_default_stage_boundaries_preserve_report_reserve(self) -> None:
        budget = RunBudget.start(total_seconds=1_800.0, now=100.0)
        self.assertEqual(budget.discovery_deadline, 700.0)
        self.assertEqual(budget.mutation_deadline, 1_600.0)
        self.assertEqual(budget.deadline, 1_900.0)
        self.assertEqual(budget.discovery_timeout(650.0), 50.0)
        self.assertEqual(budget.mutation_timeout(1_500.0), 100.0)
        self.assertFalse(budget.may_start_mutation(1_600.0))


class StoreTests(unittest.TestCase):
    def test_checkpoint_atomically_replaces_versioned_run_json(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            store = RunStore(Path(directory))
            record = RunRecord.new(total_budget_seconds=1_800.0)
            store.initialize(record)
            record.state = RunState.COMPLETED
            store.checkpoint(record)
            value = json.loads((Path(directory) / "run.json").read_text())
            self.assertEqual(value["schema_version"], 2)
            self.assertEqual(value["state"], "completed")
            self.assertFalse((Path(directory) / ".run.json.tmp").exists())
