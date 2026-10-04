"""Make missing/rewritten research evidence fail instead of silently disappearing."""
import ast
from collections import Counter
import copy
import sys
import unittest
from unittest.mock import patch

import research_python as research


class ResearchClosureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original = research.read_json(research.INVENTORY)
        cls.decisions = research.read_json(research.DECISIONS)

    def changed(self):
        return copy.deepcopy(self.original)

    def remove(self, data, predicate):
        entry = next(e for e in data["entries"] if predicate(e))
        data["entries"].remove(entry)
        # Deliberately fix the superficial count; a relational check must fail.
        data["counts"] = dict(Counter(e["kind"] for e in data["entries"]))
        data["detail_counts"] = research.detail_counts(data["entries"], data["modules"])

    def test_complete_checked_in_evidence(self):
        self.assertEqual(research.validate(self.original, self.decisions), 531)
        self.assertEqual(research.CATALOG.read_bytes(), research.render(self.original, self.decisions))

    def test_stock_eod_selection_does_not_select_other_endpoints(self):
        selected = [e for e in self.original["entries"] if research.decision_key(e) == "stock-eod"]
        self.assertEqual(Counter(e["kind"] for e in selected), {"wrapper": 1, "rpc": 1, "message": 2})
        self.assertEqual(next(e["name"] for e in selected if e["kind"] == "rpc"), "GetStockHistoryEod")
        disposition = next(d for d in self.decisions if d["id"] == "stock-eod")
        self.assertEqual(disposition["status"], "planned")
        self.assertEqual(len(disposition["requirements"]), 47)

    def test_lost_file_fails_even_with_adjusted_counts(self):
        data = self.changed()
        self.remove(data, lambda e: e["kind"] == "file")
        with self.assertRaisesRegex(ValueError, "File coverage"):
            research.validate(data, self.decisions)

    def test_lost_private_helper_fails(self):
        data = self.changed()
        self.remove(data, lambda e: e["name"] == "_fmt_time")
        with self.assertRaisesRegex(ValueError, "Symbol coverage"):
            research.validate(data, self.decisions)

    def test_lost_wrapper_fails_even_with_adjusted_symbol_index(self):
        data = self.changed()
        lost = next(e for e in data["entries"] if e["kind"] == "wrapper")
        self.remove(data, lambda e: e["id"] == lost["id"])
        data["wrapper_ids"].remove(lost["id"])
        module = next(m for m in data["modules"] if lost["id"] in m["symbol_ids"])
        module["symbol_ids"].remove(lost["id"])
        module["definition_count"] -= 1
        with self.assertRaisesRegex(ValueError, "Original wrapper mapping"):
            research.validate(data, self.decisions)

    def test_extra_rpc_must_not_be_lost(self):
        data = self.changed()
        self.remove(data, lambda e: e["kind"] == "rpc" and e["name"] == "GetCorporateActionSplit")
        with self.assertRaisesRegex(ValueError, "RPC count"):
            research.validate(data, self.decisions)

    def test_nested_map_message_is_not_optional_inventory(self):
        data = self.changed()
        self.remove(data, lambda e: e["name"] == "BetaEndpoints.QueryInfo.QueryParametersEntry")
        with self.assertRaisesRegex(ValueError, "Referenced protobuf type"):
            research.validate(data, self.decisions)

    def test_duplicate_identifier_fails(self):
        data = self.changed()
        data["entries"].append(data["entries"][0])
        with self.assertRaisesRegex(ValueError, "Duplicate inventory ID"):
            research.validate(data, self.decisions)

    def test_field_omission_fails(self):
        data = self.changed()
        next(e for e in data["entries"] if e["name"] == "Endpoints.DataValue")["fields"].pop()
        with self.assertRaisesRegex(ValueError, "Detail counts"):
            research.validate(data, self.decisions)

    def test_parameter_default_disagreement_fails(self):
        data = self.changed()
        entry = next(e for e in data["entries"] if e["name"] == "ThetaClient.stock_history_trade_quote")
        next(p for p in entry["parameters"] if p["name"] == "exclusive")["default"] = "False"
        with self.assertRaisesRegex(ValueError, "Signature/parameter"):
            research.validate(data, self.decisions)

    def test_unclassified_feature_fails(self):
        decisions = [d for d in self.decisions if d["id"] != "query-wrappers"]
        with self.assertRaisesRegex(ValueError, "Unclassified"):
            research.validate(self.original, decisions)

    def test_unsupported_claim_cannot_be_marked_supported_without_evidence(self):
        decisions = copy.deepcopy(self.decisions)
        next(d for d in decisions if d["id"] == "remote-lifecycle")["status"] = "supported"
        with self.assertRaisesRegex(ValueError, "Supported feature lacks evidence"):
            research.validate(self.original, decisions)

    def test_undefined_requirement_fails(self):
        decisions = copy.deepcopy(self.decisions)
        decisions[0]["requirements"] = ["AUTH-L3-999"]
        with self.assertRaisesRegex(ValueError, "Undefined requirement"):
            research.validate(self.original, decisions)

    def test_stale_description_is_detected_by_read_only_check(self):
        data = self.changed()
        next(e for e in data["entries"] if e["kind"] == "wrapper")["description"] += " altered"
        original_read = research.read_json

        def read(path):
            return data if path == research.INVENTORY else original_read(path)

        with patch.object(research, "read_json", side_effect=read), patch.object(sys, "argv", ["research_python.py", "--check"]):
            with self.assertRaisesRegex(ValueError, "Catalogue/coverage rows are stale"):
                research.main()

    def test_source_definition_walk_includes_conditional_and_nested_definitions(self):
        tree = ast.parse("if True:\n class C:\n  def f(self):\n   def inner(): pass\n   return inner\n")
        self.assertEqual([name for name, _ in research.symbols(tree)], ["C", "C.f", "C.f.inner"])

    def test_request_presence_keeps_false_branch_and_ignores_logging_try(self):
        tree = ast.parse("try:\n query_parameters['x'] = str(x)\nexcept Exception: pass\nif x:\n query.x = x\nelse:\n query.x = False\n")
        values = research.bindings(tree.body)
        self.assertEqual([(v["target"], v["expression"], v["when"]) for v in values],
                         [("query.x", "x", ["x"]), ("query.x", "False", ["not (x)"])])


if __name__ == "__main__":
    unittest.main()
