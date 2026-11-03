"""Keep the vendor package's log output (which includes the session token) silent.

thetadata 1.0.12-1.0.17 logs the entire successful auth response, including
``sessionId``, at INFO, and rejection bodies at ERROR (research:
docs/research/thetadata-1.0.12.md, "Logging hazard"). The harness must never
let those records reach a console, a file or any handler that a dependency
might install. This module is pure standard library and must be applied
**before** ``thetadata`` is imported.

Two layers, because the vendor's logger name is not recorded in our research:

1. The ``thetadata`` logger hierarchy: level above CRITICAL, no propagation,
   and a ``NullHandler``. Records from ``thetadata`` and every child logger
   are dropped at the logger.
2. A filter on the root logger, and on every handler present now or later
   (``logging.lastResort`` and a wrapped ``Logger.addHandler``), dropping any
   record whose source file lies inside the installed vendor package
   directory. This covers calls through the root logger (``logging.info``)
   and anything else that reaches a handler.
"""

from __future__ import annotations

import importlib.util
import logging
import os
from pathlib import Path

VENDOR = "thetadata"
_SILENT = logging.CRITICAL + 1


def vendor_directory() -> Path | None:
    """The installed package directory, located without importing the package."""
    spec = importlib.util.find_spec(VENDOR)
    if spec is None or not spec.submodule_search_locations:
        return None
    return Path(next(iter(spec.submodule_search_locations))).resolve()


class _DropVendor(logging.Filter):
    def __init__(self, directory: Path | None):
        super().__init__()
        self.prefix = (str(directory) + os.sep) if directory else None

    def filter(self, record: logging.LogRecord) -> bool:
        name = record.name or ""
        if name == VENDOR or name.startswith(VENDOR + "."):
            return False
        if self.prefix and record.pathname:
            try:
                path = str(Path(record.pathname).resolve())
            except OSError:
                path = record.pathname
            if path.startswith(self.prefix):
                return False
        return True


_GUARD: _DropVendor | None = None


def install(directory: Path | None = None) -> None:
    """Apply both layers. Idempotent."""
    global _GUARD
    if _GUARD is None:
        _GUARD = _DropVendor(directory if directory is not None else vendor_directory())
    vendor = logging.getLogger(VENDOR)
    vendor.setLevel(_SILENT)
    vendor.propagate = False
    if not any(isinstance(h, logging.NullHandler) for h in vendor.handlers):
        vendor.addHandler(logging.NullHandler())
    root = logging.getLogger()
    if _GUARD not in root.filters:
        root.addFilter(_GUARD)
    for handler in list(root.handlers) + [logging.lastResort]:
        if handler is not None and _GUARD not in handler.filters:
            handler.addFilter(_GUARD)
    # Handlers added later (for example a dependency's basicConfig) get the
    # filter too, whichever logger they are attached to.
    if not getattr(logging.Logger.addHandler, "_praevis_guarded", False):
        original = logging.Logger.addHandler

        def add_handler(self, handler):
            if _GUARD is not None and _GUARD not in handler.filters:
                handler.addFilter(_GUARD)
            return original(self, handler)

        add_handler._praevis_guarded = True
        logging.Logger.addHandler = add_handler


def self_test(directory: Path | None = None) -> None:
    """Raise if a vendor-style token log line would be emitted anywhere."""
    import io

    install(directory)
    sentinel = "sessionId=GUARD-SELF-TEST-TOKEN"
    stream = io.StringIO()
    probe = logging.StreamHandler(stream)
    probe.setLevel(logging.DEBUG)
    root = logging.getLogger()
    previous = root.level
    root.setLevel(logging.DEBUG)
    root.addHandler(probe)  # like a dependency calling basicConfig(level=INFO)
    try:
        for name in (VENDOR, VENDOR + ".client", VENDOR + "._proto"):
            logger = logging.getLogger(name)
            logger.info("auth response %s", sentinel)
            logger.error("rejected %s", sentinel)
        if _GUARD is not None and _GUARD.prefix:
            for level in (logging.INFO, logging.ERROR):
                record = logging.LogRecord("root", level, _GUARD.prefix + "client.py", 157,
                                           "auth response %s", (sentinel,), None)
                root.handle(record)
    finally:
        root.removeHandler(probe)
        root.setLevel(previous)
    if sentinel in stream.getvalue():
        raise RuntimeError("log guard failed: a vendor log record would be emitted")
