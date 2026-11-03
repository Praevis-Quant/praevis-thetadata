"""Tests for log_guard. Standard library only; the vendor package is not needed.

Run: python -m unittest test_log_guard   (from bench/python-baseline)
"""

import contextlib
import io
import logging
import tempfile
import unittest
from pathlib import Path

import log_guard

TOKEN = "sessionId=FAKE-TOKEN-1234"


class LogGuardTests(unittest.TestCase):
    def setUp(self):
        self.vendor_dir = Path(tempfile.mkdtemp()) / "thetadata"
        self.vendor_dir.mkdir()
        log_guard._GUARD = None
        log_guard.install(self.vendor_dir)

    def emitted(self, action):
        """Run `action` with a root DEBUG handler and the stderr fallback captured."""
        stream = io.StringIO()
        handler = logging.StreamHandler(stream)
        handler.setLevel(logging.DEBUG)
        root = logging.getLogger()
        previous = root.level
        root.setLevel(logging.DEBUG)
        root.addHandler(handler)  # simulates basicConfig(level=INFO) by a dependency
        stderr = io.StringIO()
        try:
            with contextlib.redirect_stderr(stderr):
                action()
        finally:
            root.removeHandler(handler)
            root.setLevel(previous)
        return stream.getvalue() + stderr.getvalue()

    def test_vendor_logger_hierarchy_is_silent(self):
        def action():
            for name in ("thetadata", "thetadata.client", "thetadata._proto.v3grpc"):
                logging.getLogger(name).info("Authenticated: %s", TOKEN)
                logging.getLogger(name).error("Rejected: %s", TOKEN)
                logging.getLogger(name).critical("Critical: %s", TOKEN)
        self.assertNotIn(TOKEN, self.emitted(action))

    def test_root_logger_calls_from_vendor_files_are_dropped(self):
        def action():
            for level in (logging.INFO, logging.ERROR, logging.CRITICAL):
                record = logging.LogRecord("root", level, str(self.vendor_dir / "client.py"),
                                           157, "auth response %s", (TOKEN,), None)
                logging.getLogger().handle(record)
        self.assertNotIn(TOKEN, self.emitted(action))

    def test_handlers_added_later_are_guarded(self):
        stream = io.StringIO()
        late = logging.StreamHandler(stream)
        logging.getLogger("some.dependency").addHandler(late)
        try:
            logging.getLogger("some.dependency").error(
                "%s", TOKEN, extra={})  # not vendor: must still be emitted
            record = logging.LogRecord("some.dependency", logging.ERROR,
                                       str(self.vendor_dir / "x.py"), 1, "%s", (TOKEN,), None)
            late.handle(record)  # vendor file reaching the handler directly
        finally:
            logging.getLogger("some.dependency").removeHandler(late)
        self.assertEqual(stream.getvalue().count(TOKEN), 1)

    def test_other_loggers_still_work(self):
        def action():
            logging.getLogger("harness").warning("visible line")
        self.assertIn("visible line", self.emitted(action))

    def test_self_test_passes(self):
        log_guard.self_test(self.vendor_dir)


if __name__ == "__main__":
    unittest.main()
