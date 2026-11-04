"""Independent, unofficial Python client for ThetaData's direct gRPC API.

A thin pass-through: queries run in Rust with the GIL released, and results
are Arrow record batches shared through the Arrow PyCapsule interface, so
pyarrow, Polars and pandas import them without copying and without one Python
object per row. Prices are exact ``decimal128(38, 10)``; timestamps are
millisecond timestamps with their time zone.

Not affiliated with Theta Data Inc.
"""
from __future__ import annotations

import os
from typing import Any

from . import _native
from ._native import Batch, BatchIterator, OutputFailed, QueryFailed, ThetaDataError, __version__

__all__ = [
    "Batch",
    "BatchIterator",
    "Client",
    "OutputFailed",
    "QueryFailed",
    "Result",
    "ThetaDataError",
    "__version__",
    "describe",
    "endpoints",
]


class Result:
    """A whole result. ``pyarrow.table(result)`` and ``polars.DataFrame(result)``
    import it without copying, through ``__arrow_c_stream__``."""

    __slots__ = ("_table",)

    def __init__(self, table: _native.Table) -> None:
        self._table = table

    def __arrow_c_stream__(self, requested_schema: Any = None) -> Any:
        return self._table.__arrow_c_stream__(requested_schema)

    def __len__(self) -> int:
        return self._table.num_rows

    @property
    def num_rows(self) -> int:
        return self._table.num_rows

    @property
    def column_names(self) -> list[str]:
        return self._table.column_names

    def batches(self) -> list[Batch]:
        return self._table.batches()

    def to_arrow(self) -> Any:
        """A ``pyarrow.Table`` sharing the Arrow buffers."""
        import pyarrow

        return pyarrow.table(self)

    def to_polars(self) -> Any:
        """A ``polars.DataFrame`` from the Arrow buffers, without copying."""
        import polars

        return polars.from_arrow(self.to_arrow())

    def to_pandas(self, *, arrow_backed: bool = True) -> Any:
        """A ``pandas.DataFrame``. By default its columns stay Arrow-backed
        (``pandas.ArrowDtype``), so exact decimals and zoned timestamps are kept
        without a Python object per value. ``arrow_backed=False`` gives NumPy
        dtypes; decimal columns then hold one ``decimal.Decimal`` per row."""
        table = self.to_arrow()
        if arrow_backed:
            import pandas

            return table.to_pandas(types_mapper=pandas.ArrowDtype)
        return table.to_pandas()


class Client:
    """A client for the direct gRPC service.

    ``Client(api_key)`` authenticates with an explicit key; ``Client.from_env()``
    reads ``THETADATA_API_KEY``. The key is never logged. The scheduler defaults
    to FREE's documented limits (1 concurrent request, 20 per minute).
    """

    __slots__ = ("_native",)

    def __init__(self, api_key: str, **options: Any) -> None:
        self._native = _native.Client(api_key, **options)

    @classmethod
    def from_env(cls, variable: str = "THETADATA_API_KEY", **options: Any) -> "Client":
        key = os.environ.get(variable, "").strip()
        if not key:
            raise ThetaDataError(f"{variable} is not set")
        return cls(key, **options)

    def query(self, endpoint: str, /, *, plan: bool = True, **params: Any) -> Result:
        """Run ``endpoint`` (a vendor name such as ``"stock_history_eod"``, or
        an RPC name) with the vendor's parameter names. With ``plan``, long
        date ranges are split within the vendor's limits and merged in order."""
        return Result(self._native.query(endpoint, plan=plan, **params))

    def batches(self, endpoint: str, /, *, plan: bool = True, **params: Any) -> BatchIterator:
        """Like ``query``, but yields batches as they arrive, with bounded memory."""
        return self._native.batches(endpoint, plan=plan, **params)

    def to_parquet(
        self,
        endpoint: str,
        path: str | os.PathLike[str],
        /,
        *,
        overwrite: bool = False,
        zstd_level: int | None = 1,
        plan: bool = True,
        **params: Any,
    ) -> tuple[int, int, int]:
        """Write the result to Parquet from Rust; returns ``(rows, row_groups,
        bytes)``. The file appears only when it is complete."""
        return self._native.to_parquet(
            endpoint, os.fspath(path), overwrite=overwrite, zstd_level=zstd_level, plan=plan, **params
        )


def describe(endpoint: str, /, *, plan: bool = True, **params: Any) -> tuple[str, int]:
    """The wire query ``endpoint`` would send, and how many parts a job plans.
    Sends nothing."""
    return _native.describe(endpoint, plan=plan, **params)


def endpoints() -> list[tuple[str, str]]:
    """Every endpoint as ``(vendor_name or "", rpc_name)``."""
    return _native.endpoints()
