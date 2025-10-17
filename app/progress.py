"""
Simple shared progress manager used by clustering pipeline and custom bisecting KMeans.

Args:
    None

Returns:
    None
"""

import sys
import os
import contextlib
import logging
from typing import Optional


def flush_print(msg: str):
    """
    Print with flush to ensure immediate stdout delivery.

    Args:
        msg (str): Message to print.

    Returns:
        None
    """
    # Always write to the original stdout so that temporary stdout/stderr
    # redirection (silencing) does not hide STATUS/PROGRESS messages.
    sys.__stdout__.write(str(msg) + "\n")
    sys.__stdout__.flush()


@contextlib.contextmanager
def silence_output():
    """
    Context manager that mutes stdout/stderr and reduces noisy logger verbosity.

    Only messages printed via flush_print(...) will still appear (they write to sys.__stdout__).
    Use this around calls that produce noisy third-party output (transformers, tqdm, torch, etc.).
    """
    devnull = open(os.devnull, "w")
    old_stdout, old_stderr = sys.stdout, sys.stderr
    root_logger = logging.getLogger()
    old_root_level = root_logger.level
    try:
        sys.stdout = devnull
        sys.stderr = devnull
        root_logger.setLevel(logging.ERROR)
        # Try to quiet transformers if available
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
    """
    Shared progress manager that tracks a single global progress value and STATUS.

    Args:
        total_units (int): Total number of atomic units of work in the whole pipeline.
        initial_status (Optional[str]): Optional initial status message.

    Returns:
        None
    """

    def __init__(self, total_units: int, initial_status: Optional[str] = None):
        self.total_units = max(1, int(total_units))
        self.current = 0
        self.status = initial_status or ""

    def set_status(self, msg: str):
        """
        Update the STATUS message (prints immediately).

        Args:
            msg (str): New status message.

        Returns:
            None
        """
        self.status = msg
        flush_print(f"STATUS: {msg}")

    def advance(self, units: int = 1):
        """
        Advance progress by given atomic units and print updated PROGRESS.

        Args:
            units (int): Number of units to advance (default: 1).

        Returns:
            None
        """
        self.current = min(self.total_units, self.current + max(0, int(units)))
        self._report_progress()

    def _report_progress(self):
        """
        Internal: compute percent from current/total and print PROGRESS.

        Args:
            None

        Returns:
            None
        """
        pct = int(round(self.current / self.total_units * 100))
        flush_print(f"PROGRESS: {pct}")

    def complete(self):
        """
        Mark progress completed and print final PROGRESS=100.

        Args:
            None

        Returns:
            None
        """
        self.current = self.total_units
        self._report_progress()
        flush_print("DONE")