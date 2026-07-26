from dataclasses import dataclass
import json
import os
from pathlib import Path
import re

from .model import RunRecord


@dataclass(frozen=True)
class CommandPaths:
    stdout: Path
    stderr: Path


class RunStore:
    def __init__(self, output: Path) -> None:
        self.output = output
        self.commands = output / "commands"

    def initialize(self, record: RunRecord) -> None:
        if self.output.name.startswith("mutants.out"):
            raise ValueError("output path must not use the mutants.out prefix")
        if self.output.exists() and not self.output.is_dir():
            raise ValueError("output path exists and is not a directory")
        self.commands.mkdir(parents=True, exist_ok=True)
        self.checkpoint(record)

    def checkpoint(self, record: RunRecord) -> None:
        temporary = self.output / ".run.json.tmp"
        destination = self.output / "run.json"
        with temporary.open("w", encoding="utf-8") as stream:
            json.dump(record.to_dict(), stream, sort_keys=True, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        temporary.replace(destination)

    def command_paths(self, sequence: int, label: str) -> CommandPaths:
        safe_label = re.sub(r"[^A-Za-z0-9_.-]+", "-", label).strip(".-") or "command"
        stem = f"{sequence:04d}-{safe_label}"
        return CommandPaths(
            stdout=self.commands / f"{stem}.stdout",
            stderr=self.commands / f"{stem}.stderr",
        )
