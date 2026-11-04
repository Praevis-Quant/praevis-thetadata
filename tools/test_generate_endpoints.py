"""Tests for generate_endpoints.py (standard library only; no network)."""
import json
import tempfile
import unittest
from pathlib import Path

import generate_endpoints as gen

PARAMS = {"symbol", "start_date", "min_time", "expiration", "venue"}


class TranslationTests(unittest.TestCase):
    def test_every_known_expression_pattern(self):
        cases = {
            "venue": ("venue", "direct"),
            "start_date.strftime('%Y-%m-%d')": ("start_date", "date"),
            "_fmt_time(min_time)": ("min_time", "time"),
            "_fmt_if_date(expiration)": ("expiration", "expiration"),
            "query.symbol.extend([symbol] if isinstance(symbol, str) else symbol)": ("symbol", "extend"),
        }
        for expression, expected in cases.items():
            with self.subTest(expression=expression):
                self.assertEqual(gen.classify_expression(expression, PARAMS), expected)

    def test_unknown_expressions_and_defaults_are_errors(self):
        for expression in ("venue.upper()", "unknown", "str(symbol)", "_fmt_time(other)"):
            with self.subTest(expression=expression), self.assertRaises(gen.GenerationError):
                gen.classify_expression(expression, PARAMS)
        for kind, literal in (("String", "1"), ("i32", "'x'"), ("bool", "1"), ("f64", "foo()")):
            with self.subTest(kind=kind, literal=literal), self.assertRaises(gen.GenerationError):
                gen.default_expression(kind, literal)

    def test_defaults(self):
        self.assertEqual(gen.default_expression("String", "'nqb'"), '"nqb".to_owned()')
        self.assertEqual(gen.default_expression("TimeOfDay", "_fmt_time('09:30:00')"),
                         'TimeOfDay::Text("09:30:00".to_owned())')
        self.assertEqual(gen.default_expression("bool", "False"), "false")
        self.assertEqual(gen.default_expression("i32", "101"), "101")
        self.assertEqual(gen.default_expression("f64", "101"), "101.0")

    def test_contract_spec(self):
        spec = gen.parse_spec("ContractSpec(symbol=symbol, expiration=_fmt_if_date(expiration), "
                              "strike=venue, right=venue)", PARAMS)
        self.assertEqual(spec.params["expiration"], "expiration")
        self.assertEqual(spec.transforms["expiration"], "expiration")

    def test_names(self):
        self.assertEqual(gen.snake("GetStockHistoryEod"), "get_stock_history_eod")
        self.assertEqual(gen.snake("GetOptionHistoryGreeksEod"), "get_option_history_greeks_eod")
        self.assertEqual(gen.camel("option_history_trade_quote"), "OptionHistoryTradeQuote")
        self.assertEqual(gen.ident("type"), "r#type")

    def test_an_argument_that_never_reaches_the_wire_is_an_error(self):
        wrapper = {"name": "ThetaClient.x", "parameters": [
            {"name": "self", "annotation": None, "default": None},
            {"name": "unused", "annotation": "str", "default": None}],
            "bindings": [], "description": ""}
        with self.assertRaises(gen.GenerationError):
            gen.translate_wrapper({"name": "GetX"}, wrapper, [])


class OutputTests(unittest.TestCase):
    def test_committed_outputs_are_up_to_date(self):
        rust, doc = gen.generate()
        self.assertEqual(gen.RUST_OUT.read_text(encoding="utf-8"), rust)
        self.assertEqual(gen.DOC_OUT.read_text(encoding="utf-8"), doc)
        self.assertEqual(rust.count("impl Endpoint for"), 100)

    def test_status_entries_must_name_real_rpcs(self):
        with tempfile.TemporaryDirectory() as directory:
            status = Path(directory) / "status.json"
            status.write_text(json.dumps({"GetNoSuchRpc": {"decoder": "x"}}))
            with self.assertRaises(gen.GenerationError):
                gen.generate(status_path=status)

    def test_free_results_are_codes_and_classifications_only(self):
        ok = {"class": "tier-denied", "code": "PermissionDenied", "tier": "VALUE", "run": "p1-ent-001"}
        with tempfile.TemporaryDirectory() as directory:
            status = Path(directory) / "status.json"
            status.write_text(json.dumps({"GetStockHistoryTrade": {"free": ok}}))
            _, doc = gen.generate(status_path=status)
            self.assertIn("| tier-denied: VALUE (`PermissionDenied`, p1-ent-001) |", doc)
            bad = [
                {**ok, "text": "vendor message"},             # extra field
                {**ok, "class": "requires a VALUE plan"},      # free text as class
                {**ok, "code": "permission denied"},           # not a gRPC code name
                {**ok, "tier": "Value subscription"},          # not a tier name
                {**ok, "class": "entitled", "code": "OK"},     # tier only on tier-denied
                {"class": "entitled", "code": "OK"},           # missing run
                {**ok, "run": "../x y"},                       # invalid run id
            ]
            for entry in bad:
                status.write_text(json.dumps({"GetStockHistoryTrade": {"free": entry}}))
                with self.subTest(entry=entry), self.assertRaises(gen.GenerationError):
                    gen.generate(status_path=status)
            status.write_text(json.dumps({"GetStockHistoryTrade": {"live": "vendor text"}}))
            with self.assertRaises(gen.GenerationError):
                gen.generate(status_path=status)


if __name__ == "__main__":
    unittest.main()
