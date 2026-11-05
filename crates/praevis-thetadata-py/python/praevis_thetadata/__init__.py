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
from ._native import Batch, BatchIterator, OutputFailed, QueryFailed, SessionEnded, ThetaDataError, __version__
from ._vendor import VendorMethods

__all__ = [
    "Batch",
    "BatchIterator",
    "Client",
    "OutputFailed",
    "QueryFailed",
    "SessionEnded",
    "Result",
    "ThetaDataError",
    "VendorMethods",
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


_DATAFRAME_TYPES = ("polars", "pandas", "arrow")
_CREDENTIAL_SOURCES = (
    "supply creds_file, api_key, or email and password; or set THETADATA_API_KEY, "
    "or THETADATA_EMAIL and THETADATA_PASSWORD, or THETADATA_CREDENTIALS_FILE; "
    "or create ./creds.txt"
)


def _credentials(
    creds_file: str | os.PathLike[str] | None,
    api_key: str | None,
    email: str | None,
    password: str | None,
) -> dict[str, Any]:
    """Choose one source in the ADR-0020 order, the CLI's and the vendor's:
    explicit arguments (credentials file, API key, email and password), then
    THETADATA_API_KEY, a complete THETADATA_EMAIL / THETADATA_PASSWORD pair,
    THETADATA_CREDENTIALS_FILE and finally ./creds.txt. Blank values fall
    through. Nothing here is logged or kept."""
    if creds_file is not None:
        return {"creds_file": os.fspath(creds_file)}
    if api_key is not None and api_key.strip():
        return {"api_key": api_key}
    if email is not None or password is not None:
        if not (email and email.strip() and password):
            raise ThetaDataError("give both email and password")
        return {"email": email, "password": password}
    key = os.environ.get("THETADATA_API_KEY", "")
    if key.strip():
        return {"api_key": key}
    env_email = os.environ.get("THETADATA_EMAIL", "")
    env_password = os.environ.get("THETADATA_PASSWORD", "")
    if env_email.strip() and env_password:
        return {"email": env_email, "password": env_password}
    selected = os.environ.get("THETADATA_CREDENTIALS_FILE", "")
    if selected:
        # A selected file is used even if missing, so its error is reported.
        return {"creds_file": selected}
    if os.path.exists("creds.txt"):
        return {"creds_file": "creds.txt"}
    if bool(env_email.strip()) != bool(env_password):
        raise ThetaDataError(
            "only one of THETADATA_EMAIL and THETADATA_PASSWORD is set, and no ./creds.txt was found"
        )
    raise ThetaDataError("no credentials: " + _CREDENTIAL_SOURCES)


class Client(VendorMethods):
    """A client for the direct gRPC service, with the vendor ``ThetaClient``
    method names (see ``VendorMethods``) and the enhanced ``query``,
    ``batches`` and ``to_parquet``.

    Credentials follow the vendor and CLI order (ADR-0020): explicit
    ``creds_file``, ``api_key``, or ``email`` and ``password``; otherwise
    ``THETADATA_API_KEY``, ``THETADATA_EMAIL`` and ``THETADATA_PASSWORD``,
    ``THETADATA_CREDENTIALS_FILE`` and ``./creds.txt``. They are never logged.

    ``dataframe_type`` (``"polars"``, the vendor default, ``"pandas"`` or
    ``"arrow"``) is what the vendor-named methods return. The scheduler
    follows the account's tier from the login response (``tier``): FREE 1
    concurrent request and 20 per minute; VALUE 2, STANDARD 4 and PRO 8
    concurrent requests with no per-minute limit. ``concurrency=`` and
    ``per_minute=`` override it (``per_minute=0`` removes the limit);
    ``scheduler`` shows the limits in use.

    **One session per account** (confirmed by ``opt-session-001``). Each login
    starts a new session, and the service ends the account's earlier one: a
    second ``Client`` that logs in (or a CLI or vendor-package login) makes
    the first one's requests fail with ``SessionEnded`` (gRPC
    ``UNAUTHENTICATED``). The client neither retries nor logs in again by
    itself. Log in once and share it: ``Client(existing_authorized_client=client,
    dataframe_type="pandas")`` reuses ``client``'s session, connection and
    scheduler, as in the vendor package.
    """

    __slots__ = ("_native", "_dataframe_type")

    def __init__(
        self,
        api_key: str | None = None,
        *,
        email: str | None = None,
        password: str | None = None,
        creds_file: str | os.PathLike[str] | None = None,
        existing_authorized_client: "Client | None" = None,
        dataframe_type: str = "polars",
        **options: Any,
    ) -> None:
        if dataframe_type not in _DATAFRAME_TYPES:
            raise ValueError(f"dataframe_type: one of {', '.join(_DATAFRAME_TYPES)}")
        if existing_authorized_client is not None:
            if any(v is not None for v in (api_key, email, password, creds_file)) or options:
                raise ValueError("existing_authorized_client shares a session: give no credentials or options")
            # Same session, connection and scheduler; no second login.
            self._native = existing_authorized_client._native
        else:
            credentials = _credentials(creds_file, api_key, email, password)
            self._native = _native.Client(**credentials, **options)
        self._dataframe_type = dataframe_type

    @classmethod
    def from_env(cls, **options: Any) -> "Client":
        """A client from the environment or ./creds.txt, in the ADR-0020 order."""
        return cls(**options)

    @property
    def dataframe_type(self) -> str:
        return self._dataframe_type

    @property
    def tier(self) -> str:
        """The account's tier from the login response, such as ``"PRO"``."""
        return self._native.tier

    @property
    def scheduler(self) -> tuple[int, int | None]:
        """``(concurrency, per_minute)`` in use; ``None`` means no per-minute limit."""
        return self._native.scheduler

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

    def _vendor(self, name: str, plan: bool, params: dict[str, Any]) -> Any:
        result = Result(self._native.query(name, plan=plan, **params))
        if self._dataframe_type == "polars":
            return result.to_polars()
        if self._dataframe_type == "pandas":
            return result.to_pandas()
        return result.to_arrow()


def describe(endpoint: str, /, *, plan: bool = True, **params: Any) -> tuple[str, int]:
    """The wire query ``endpoint`` would send, and how many parts a job plans.
    Sends nothing."""
    return _native.describe(endpoint, plan=plan, **params)


def endpoints() -> list[tuple[str, str]]:
    """Every endpoint as ``(vendor_name or "", rpc_name)``."""
    return _native.endpoints()
