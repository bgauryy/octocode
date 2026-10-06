"""Monotonic heartbeat cadence shared by long-running presence loops."""
import os
import time

HEARTBEAT_ENV = 'OCTOCODE_COMMUNICATION_HEARTBEAT_MS'
DEFAULT_HEARTBEAT_MS = 15000
MIN_HEARTBEAT_MS, MAX_HEARTBEAT_MS = 50, 60000


def heartbeat_seconds():
    """Heartbeat period; the environment override exists so tests need not wait real periods."""
    raw = os.environ.get(HEARTBEAT_ENV, '').strip()
    if not raw:
        return DEFAULT_HEARTBEAT_MS / 1000
    if not (raw.isascii() and raw.isdigit()) or not MIN_HEARTBEAT_MS <= int(raw) <= MAX_HEARTBEAT_MS:
        raise ValueError('%s must be an integer from %d to %d' % (HEARTBEAT_ENV, MIN_HEARTBEAT_MS, MAX_HEARTBEAT_MS))
    return int(raw) / 1000


def deadline_after(duration_ms):
    """Monotonic deadline for an optional duration, or None when unbounded."""
    return None if duration_ms is None else time.monotonic() + duration_ms / 1000


def running(stop, deadline):
    """True until the stop event fires or the optional monotonic deadline passes."""
    return not stop.is_set() and (deadline is None or time.monotonic() < deadline)


class Heartbeat:
    """Runs a beat once per heartbeat period, measured from the end of the previous beat."""

    def __init__(self, beat, due_now=False):
        self.beat = beat
        self.interval = heartbeat_seconds()
        self.last = float('-inf') if due_now else time.monotonic()

    def reset(self):
        self.last = time.monotonic()

    def tick(self, force=False):
        if force or time.monotonic() - self.last >= self.interval:
            self.beat()
            self.reset()
            return True
        return False
