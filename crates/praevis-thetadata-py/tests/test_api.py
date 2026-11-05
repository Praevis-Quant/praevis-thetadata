"""Endpoint names, keyword arguments and wire queries (no network)."""
import datetime as dt
import unittest

import praevis_thetadata as pt


class EndpointTests(unittest.TestCase):
    def test_every_rpc_is_reachable_and_81_have_vendor_names(self):
        names = pt.endpoints()
        self.assertEqual(len(names), 100)
        self.assertEqual(len({rpc for _, rpc in names}), 100)
        self.assertEqual(sum(1 for vendor, _ in names if vendor), 81)

    def test_vendor_and_rpc_names_build_the_same_query(self):
        kwargs = dict(symbol="AAPL", start_date=dt.date(2024, 1, 2), end_date="2024-01-05")
        by_vendor = pt.describe("stock_history_eod", **kwargs)
        by_rpc = pt.describe("GetStockHistoryEod", **kwargs)
        self.assertEqual(by_vendor, by_rpc)
        self.assertEqual(
            by_vendor,
            ('StockHistoryEodRequestQuery { symbol: "AAPL", start_date: "2024-01-02", end_date: "2024-01-05" }', 1),
        )

    def test_plans_split_long_ranges(self):
        _, parts = pt.describe("stock_history_eod", symbol="AAPL", start_date="2022-01-01", end_date="2024-03-01")
        self.assertEqual(parts, 3)
        _, parts = pt.describe(
            "option_history_trade", symbol="SPY", expiration="2024-06-21",
            start_date="2024-06-12", end_date="2024-06-14",
        )
        self.assertEqual(parts, 3)
        _, parts = pt.describe("stock_history_eod", plan=False, symbol="AAPL", start_date="2022-01-01", end_date="2024-03-01")
        self.assertEqual(parts, 1)

    def test_vendor_defaults_none_and_symbol_lists(self):
        query, _ = pt.describe("stock_snapshot_ohlc", symbol=["AAPL", "MSFT"])
        self.assertIn('symbol: ["AAPL", "MSFT"]', query)
        self.assertIn('venue: Some("nqb")', query)  # vendor default
        query, _ = pt.describe("stock_snapshot_ohlc", symbol="AAPL", venue=None)
        self.assertIn("venue: None", query)  # explicit None clears, as in the vendor wrapper
        query, _ = pt.describe("option_history_ohlc", symbol="SPY", expiration=dt.date(2024, 6, 21),
                               start_time=dt.time(9, 30, 0, 123456), date="2024-06-14")
        self.assertIn('start_time: Some("09:30:00.123")', query)
        self.assertIn('date: Some("2024-06-14")', query)
        query, _ = pt.describe("option_history_eod", symbol="SPY", expiration="*",
                               start_date="2024-06-14", end_date="2024-06-14")
        self.assertIn('expiration: "*"', query)

    def test_argument_errors_are_python_errors(self):
        with self.assertRaises(TypeError):
            pt.describe("stock_history_eod", symbol="AAPL", start_date="2024-01-02")  # missing end_date
        with self.assertRaises(TypeError):
            pt.describe("stock_history_eod", symbol="AAPL", start_date="2024-01-02", end_date="2024-01-03", bogus=1)
        with self.assertRaises(ValueError):
            pt.describe("stock_history_eod", symbol="AAPL", start_date="01/02/2024", end_date="2024-01-03")
        with self.assertRaises(TypeError):
            pt.describe("stock_history_eod", symbol="AAPL", start_date=dt.datetime(2024, 1, 2), end_date="2024-01-03")
        with self.assertRaises(TypeError):
            pt.describe("option_history_eod", symbol="SPY", expiration="*", start_date="2024-06-14",
                        end_date="2024-06-14", max_dte=True)
        with self.assertRaises(ValueError):
            pt.describe("no_such_endpoint")



if __name__ == "__main__":
    unittest.main()


class SchedulerTests(unittest.TestCase):
    def test_limits_follow_the_tier_unless_overridden(self):
        from praevis_thetadata import _native

        tier = _native._scheduler_policy
        self.assertEqual(tier("FREE"), (1, 20))
        self.assertEqual(tier("VALUE"), (2, None))
        self.assertEqual(tier("STANDARD"), (4, None))
        self.assertEqual(tier("PRO"), (8, None))
        self.assertEqual(tier("PRO", concurrency=3), (3, None))
        self.assertEqual(tier("FREE", concurrency=3, per_minute=40), (3, 40))
        self.assertEqual(tier("FREE", per_minute=0), (1, None))
        self.assertEqual(tier("PRO", per_minute=100), (8, 100))
        with self.assertRaises(ValueError):
            tier("GOLD")


class ErrorTests(unittest.TestCase):
    def test_an_ended_session_is_a_typed_query_failure(self):
        self.assertTrue(issubclass(pt.SessionEnded, pt.QueryFailed))
        self.assertTrue(issubclass(pt.QueryFailed, pt.ThetaDataError))
        self.assertIn("existing_authorized_client", pt.SessionEnded.__doc__)
