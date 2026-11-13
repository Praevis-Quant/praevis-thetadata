"""The runnable consumer examples, against the synthetic loopback service.

Skipped unless ``PRAEVIS_EMULATOR`` names the built emulator binary
(``praevis-thetadata-emulator``); CI builds it and sets the variable, so there
they always run.
"""
import os
import pathlib
import sys
import unittest

import praevis_thetadata as pt

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "examples"))
import consumer_patterns as examples  # noqa: E402

SERVER = os.environ.get("PRAEVIS_EMULATOR")


@unittest.skipUnless(SERVER, "PRAEVIS_EMULATOR is not set")
class ConsumerExampleTests(unittest.TestCase):
    def test_a_failed_stream_keeps_its_rows_as_partial(self):
        with examples.loopback("partial") as service:
            client = service.client
            self.assertEqual(examples.keep_partial(client), ("partial", 4, "remote"))
            with self.assertRaises(pt.QueryFailed) as raised:
                client.query("stock_history_eod", symbol="SYNTHETIC", start_date="2026-01-15", end_date="2026-01-15")
            self.assertEqual(raised.exception.args[0], "remote")
            self.assertEqual(raised.exception.args[2], "Internal")

    def test_cancelling_a_job_sends_no_more_parts(self):
        with examples.loopback("slow") as service:
            first = examples.cancel_midway(service.client)
            # Six yearly parts, two at a time. A job still running would have
            # started parts 3 and 4 when the first two finished (about 1 s).
            self.assertEqual(service.requests(), 2)
        self.assertEqual(first, 3)


ONE_DAY = dict(symbol="SYNTHETIC", start_date="2026-01-15", end_date="2026-01-15")


@unittest.skipUnless(SERVER, "PRAEVIS_EMULATOR is not set")
class NoPolicyTests(unittest.TestCase):
    """ADR-0021: no retry, no default deadline; a caller's timeout still applies."""

    def test_a_busy_answer_raises_busy_and_is_not_retried(self):
        with examples.loopback("busy") as service:
            with self.assertRaises(pt.QueryFailed) as raised:
                service.client.query("stock_history_eod", **ONE_DAY)
            self.assertEqual(raised.exception.args[0], "busy")
            self.assertEqual(raised.exception.args[2], "ResourceExhausted")
            self.assertEqual(service.requests(), 1)
            # The vendor-named method too.
            with self.assertRaises(pt.QueryFailed):
                service.client.stock_history_eod("SYNTHETIC", "2026-01-15", "2026-01-15")
            self.assertEqual(service.requests(), 2)

    def test_no_deadline_applies_unless_one_is_set(self):
        with examples.loopback("delayed") as service:
            # 1.5 s before the first answer; nothing limits the call by default.
            result = service.client.query("stock_history_eod", **ONE_DAY)
            self.assertEqual(result.num_rows, 2)
            with self.assertRaises(pt.QueryFailed) as raised:
                service.client.query("stock_history_eod", timeout=0.3, **ONE_DAY)
            self.assertEqual(raised.exception.args[0], "deadline")
        # A client-wide timeout applies to the vendor-named methods.
        with examples.loopback("delayed", request_timeout=0.3) as service:
            with self.assertRaises(pt.QueryFailed) as raised:
                service.client.stock_history_eod("SYNTHETIC", "2026-01-15", "2026-01-15")
            self.assertEqual(raised.exception.args[0], "deadline")


class LoopbackSeamTests(unittest.TestCase):
    def test_the_seam_accepts_only_loopback_urls_and_both_together(self):
        from praevis_thetadata import _native

        for urls in ({"_auth_url": "http://127.0.0.1:1/auth"},
                     {"_auth_url": "http://example.com/auth", "_endpoint": "http://127.0.0.1:1"},
                     {"_auth_url": "https://127.0.0.1:1/auth", "_endpoint": "http://127.0.0.1:1"},
                     # Look-alike hosts are other hosts.
                     {"_auth_url": "http://127.0.0.1.example.com/auth", "_endpoint": "http://127.0.0.1:1"},
                     {"_auth_url": "http://localhost:1/auth", "_endpoint": "http://localhost.example.com:1"},
                     {"_auth_url": "http://user@example.com/auth", "_endpoint": "http://127.0.0.1:1"},
                     # Userinfo that looks like a loopback host: the real host is example.com.
                     {"_auth_url": "http://localhost:1@example.com/auth", "_endpoint": "http://127.0.0.1:1"},
                     {"_auth_url": "http://127.0.0.1:1/auth", "_endpoint": "http://127.0.0.1@example.com"}):
            with self.subTest(urls=urls), self.assertRaises(ValueError):
                _native.Client(api_key="synthetic", **urls)


if __name__ == "__main__":
    unittest.main()
