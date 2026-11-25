"""Check the user guide's numeric examples without importing the vendor package."""

import doctest


def load_tests(loader, tests, pattern):
    tests.addTests(doctest.DocFileSuite("../docs/user-guide.md", encoding="utf-8"))
    return tests
