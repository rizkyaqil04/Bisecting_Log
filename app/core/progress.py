# app/core/progress.py

"""Shared progress utilities."""
import sys
import os
import contextlib
import logging
from typing import Optional


def flush_print(msg: str):
    sys.__stdout__.write(str(msg) + "\n")
    sys.__stdout__.flush()


@contextlib.contextmanager
def silence_output():
    devnull = open(os.devnull, "w")
    old_stdout, old_stderr = sys.stdout, sys.stderr
    root_logger = logging.getLogger()
    old_root_level = root_logger.level
    try:
        sys.stdout = devnull
        sys.stderr = devnull
        root_logger.setLevel(logging.ERROR)
        try:
            from transformers import logging as transformers_logging
            transformers_logging.set_verbosity_error()
        except Exception:
            pass
        yield
    finally:
        sys.stdout = old_stdout
        sys.stderr = old_stderr
        root_logger.setLevel(old_root_level)
        devnull.close()


class ProgressManager:
    def __init__(self, total_units: int, initial_status: Optional[str] = None):
        self.total_units = max(1, int(total_units))
        self.current = 0
        self.status = initial_status or ""

    def set_status(self, msg: str):
        self.status = msg
        flush_print(f"STATUS: {msg}")

    def advance(self, units: int = 1):
        self.current = min(self.total_units, self.current + max(0, int(units)))
        self._report_progress()

    def _report_progress(self):
        pct = int(round(self.current / self.total_units * 100))
        flush_print(f"PROGRESS: {pct}")

    def complete(self):
        self.current = self.total_units
        self._report_progress()
        flush_print("DONE")
