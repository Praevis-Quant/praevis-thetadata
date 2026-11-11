"""The runnable consumer examples, against the synthetic loopback service.

Skipped unless ``PRAEVIS_LOOPBACK_SERVER`` names the built ``loopback_server``
example; CI builds it and sets the variable, so there they always run.
"""
import os
import pathlib
import sys
import unittest

import praevis_thetadata as pt

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "examples"))
import consumer_patterns as examples  # noqa: E402

SERVER = os.environ.get("PRAEVIS_LOOPBACK_SERVER")


@unittest.skipUnless(SERVER, "PRAEVIS_LOOPBACK_SERVER is not set")
class ConsumerExampleTests(unittest.TestCase):
    def test_a_failed_stream_keeps_its_rows_as_partial(self):
        with examples.loopback("partial") as client:
            self.assertEqual(examples.keep_partial(client), ("partial", 4, "remote"))
            with self.assertRaises(pt.QueryFailed) as raised:
                client.query("stock_history_eod", symbol="SYNTHETIC", start_date="2026-01-15", end_date="2026-01-15")
            self.assertEqual(raised.exception.args[0], "remote")
            self.assertEqual(raised.exception.args[2], "Internal")

    def test_cancelling_a_job_releases_its_slot(self):
        with examples.loopback("slow") as client:
            first, waited = examples.cancel_midway(client)
        self.assertEqual(first, 3)
        # Without the cancellation the next request would wait for the job's
        # part to finish its remaining 19 slow batches (about 3.8 s).
        self.assertLess(waited, 1.0)


class LoopbackSeamTests(unittest.TestCase):
    def test_the_seam_accepts_only_loopback_urls_and_both_together(self):
        from praevis_thetadata import _native

        for urls in ({"_auth_url": "http://127.0.0.1:1/auth"},
                     {"_auth_url": "http://example.com/auth", "_endpoint": "http://127.0.0.1:1"},
                     {"_auth_url": "https://127.0.0.1:1/auth", "_endpoint": "http://127.0.0.1:1"}):
            with self.subTest(urls=urls), self.assertRaises(ValueError):
                _native.Client(api_key="synthetic", **urls)


if __name__ == "__main__":
    unittest.main()
