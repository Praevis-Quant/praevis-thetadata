"""Vendor-named methods: exact 1.0.17 signatures, every argument forwarded,
forwarding defaults identical to omitting them, and dataframe_type (no network)."""
import ast
import datetime as dt
import inspect
import json
import pathlib
import unittest

import praevis_thetadata as pt
from praevis_thetadata import _native

INVENTORY = pathlib.Path(__file__).resolve().parents[3] / "docs/research/python-1.0.17-inventory.json"
WRAPPERS = [e for e in json.loads(INVENTORY.read_text(encoding="utf-8"))["entries"] if e["kind"] == "wrapper"]

# Realistic values for required parameters, by annotation.
SAMPLE = {
    "str": "SPY",
    "datetime.date": dt.date(2024, 6, 14),
    "Union[datetime.date, str]": dt.date(2024, 6, 21),
    "Union[datetime.time, str]": "10:00:00",
    "Union[List[str], str]": ["SPY"],
}
NAMED = {"request_type": "trade", "year": "2024"}


def vendor_default(literal):
    if literal.startswith("_fmt_time("):
        return ast.literal_eval(literal[len("_fmt_time("):-1])
    return ast.literal_eval(literal)


class Recorder:
    """Stands in for the native client: records calls, returns rows."""

    def __init__(self):
        self.calls = []

    def query(self, name, *, plan, **params):
        self.calls.append((name, plan, params))
        return _native._synthetic(3)


def offline_client(dataframe_type="arrow"):
    client = object.__new__(pt.Client)
    recorder = Recorder()
    object.__setattr__(client, "_native", recorder)
    object.__setattr__(client, "_dataframe_type", dataframe_type)
    return client, recorder


def required(wrapper):
    return {p["name"]: NAMED.get(p["name"], SAMPLE[p["annotation"]])
            for p in wrapper["parameters"] if p["name"] != "self" and p["default"] is None}


class VendorSignatures(unittest.TestCase):
    def test_all_81_methods_have_the_vendor_signature(self):
        self.assertEqual(len(WRAPPERS), 81)
        for wrapper in WRAPPERS:
            method = wrapper["name"].split(".")[-1]
            with self.subTest(method):
                parameters = list(inspect.signature(getattr(pt.Client, method)).parameters.values())
                vendor = [p for p in wrapper["parameters"] if p["name"] != "self"]
                ours = parameters[1:-1]  # self ... plan
                self.assertEqual([p.name for p in ours], [p["name"] for p in vendor])
                for ours_p, vendor_p in zip(ours, vendor):
                    self.assertEqual(ours_p.kind, inspect.Parameter.POSITIONAL_OR_KEYWORD)
                    expected = inspect.Parameter.empty if vendor_p["default"] is None else vendor_default(vendor_p["default"])
                    self.assertEqual(ours_p.default, expected)
                    self.assertEqual(ours_p.annotation, vendor_p["annotation"] or inspect.Parameter.empty)
                self.assertEqual((parameters[-1].name, parameters[-1].kind), ("plan", inspect.Parameter.KEYWORD_ONLY))

    def test_every_argument_is_forwarded_by_name(self):
        for wrapper in WRAPPERS:
            method = wrapper["name"].split(".")[-1]
            with self.subTest(method):
                client, recorder = offline_client()
                values = required(wrapper)
                getattr(client, method)(**values)
                name, plan, params = recorder.calls[-1]
                self.assertEqual((name, plan), (method, True))
                vendor = [p["name"] for p in wrapper["parameters"] if p["name"] != "self"]
                self.assertEqual(list(params), vendor)
                for key, value in values.items():
                    self.assertEqual(params[key], value)

    def test_forwarding_the_defaults_builds_the_same_wire_query_as_omitting_them(self):
        for wrapper in WRAPPERS:
            method = wrapper["name"].split(".")[-1]
            with self.subTest(method):
                client, recorder = offline_client()
                values = required(wrapper)
                getattr(client, method)(**values)
                _, _, forwarded = recorder.calls[-1]
                self.assertEqual(pt.describe(method, **forwarded), pt.describe(method, **values))


class DataframeTypes(unittest.TestCase):
    def test_each_dataframe_type(self):
        import polars as pl
        import pandas as pd
        import pyarrow as pa

        for kind, expected in [("polars", pl.DataFrame), ("pandas", pd.DataFrame), ("arrow", pa.Table)]:
            client, _ = offline_client(kind)
            frame = client.stock_list_symbols()
            self.assertIsInstance(frame, expected, kind)
            self.assertEqual(len(frame), 3)

    def test_unknown_dataframe_type_is_refused_before_connecting(self):
        with self.assertRaises(ValueError):
            pt.Client(api_key="unused", dataframe_type="numpy")


if __name__ == "__main__":
    unittest.main()
