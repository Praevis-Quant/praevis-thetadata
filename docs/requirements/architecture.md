# Architecture requirements

Status: Defined/proposed, 2026-10-04; implementation and acceptance planned.
Origin of every ARCH requirement: **Rust enhancement**. Owner: workspace
maintainers; component responsibility is named in L3. These supplement the
auth/EOD contracts without claiming a refactor has occurred.
Decision: [ADR-0013](../adr/0013-library-boundaries.md).
Evidence: [review](../research/architecture-performance-review.md).
Classification: [origin register](origins.md).

## L1

| ID | Requirement | Planned success measure |
| --- | --- | --- |
| ARCH-L1-001 | Consumers shall reuse cohesive Rust interfaces independently of application, storage, and language-binding policy. | Native and headless consumer builds exercise only selected capabilities. |
| ARCH-L1-002 | Maintainers shall contain vendor and presentation changes at explicit adapter boundaries without imposing unnecessary data-path indirection. | Dependency/API review and before/after measurements show preserved domain contracts and documented adapter cost. |

## L2

| ID | L1 parents | Requirement | Planned verification |
| --- | --- | --- | --- |
| ARCH-L2-001 | ARCH-L1-001, ARCH-L1-002 | Core values/operations shall remain independent of transport, auth, CLI, persistence, environment discovery and Python; client adapters shall map vendor representation into them. | Dependency graph and compile checks for the isolated core; mapping fixture tests. |
| ARCH-L2-002 | ARCH-L1-001 | Native session persistence shall be optional for library consumers while explicitly enabled by the CLI with unchanged auth behavior. | Minimal/default/CLI feature builds and native-store process tests on Windows/Linux. |
| ARCH-L2-003 | ARCH-L1-002 | Request, wire, decoding, stream lifecycle, and output responsibilities shall have distinct module/API boundaries; new crates shall require independent responsibility/reuse or measured dependency benefit. | Design/API review; no transport or generated types in the stable numeric core model. |

## L3

| ID | L2 parents | Owner | Requirement | Planned verification |
| --- | --- | --- | --- | --- |
| ARCH-L3-001 | ARCH-L2-001 | core, client | New numeric batches/pure transforms shall use no HTTP, gRPC, runtime, keyring, CLI, PyO3 or filesystem dependencies. Vendor price/zone interpretation shall occur at client conversion; existing public core helpers may remain compatibility wrappers. | Cargo dependency audit, isolated core tests and vendor mapping fixtures. |
| ARCH-L3-002 | ARCH-L2-002 | auth, CLI | Add an explicit native-store feature, with a storage-disabled auth build excluding keyring and its platform services. The CLI explicitly selects persistence; preserve existing default library API compatibility until a separately documented migration changes defaults. | Separate consumer builds avoid Cargo feature-unification masking; inspect resolved dependencies and run unchanged CLI persistence tests. |
| ARCH-L3-003 | ARCH-L2-001, ARCH-L2-003 | client | Typed and raw APIs shall be visibly separate; generated schema changes shall be adapted in request/codec modules. Core/public enhanced values shall not wrap generated structs as their required representation. | Compile consumers of each interface; synthetic wire-to-domain fixtures. |
| ARCH-L3-004 | ARCH-L2-001, ARCH-L2-003 | workspace | Host adapters shall own config discovery, runtime construction and presentation. Future bindings shall depend inward on reusable Rust interfaces with no dependency back from core/auth/client to the binding. | Dependency-direction/build tests when each adapter is selected; existing auth runtime/CLI checks. |
| ARCH-L3-005 | ARCH-L2-003 | client, workspace | Refactors shall retain one EOD validation/transport/lifecycle engine shared by numeric and Table adapters; avoid per-asset crates or per-cell dynamic dispatch without an evidenced need. | API/control-flow review, both consumer fixtures and adapter-overhead benchmarks. |
