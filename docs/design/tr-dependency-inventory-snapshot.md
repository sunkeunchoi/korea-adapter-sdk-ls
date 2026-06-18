# TR Dependency Inventory Snapshot

**Status:** maintained migration snapshot index. The full machine-readable
snapshot lives at
`docs/migration-source/tr-dependencies-2026-06-14.json`.

This note extracts the dependency knowledge from the `korea-broker-sdk-ls`
Migration Source without reviving the old generated API surface. It is an input
for future maintained-SDK expansion, queue planning, and order-safety work. It
is not SDK Reference Docs and it is not a promise to implement every TR.

## Snapshot Identity

- Source spec hash: `238beb842b1a`
- Derivation date: `2026-06-14`
- TR count: 365
- REST TRs: 249
- WebSocket TRs: 116
- Standalone TRs: 38
- TRs with `cts_*` SELF continuation fields: 61
- Trading-day-sensitive TRs: 73
- Paper-incompatible TRs observed in the Migration Source: 7
- Manual-only order TRs: 17
- STRONG cross-TR edges: 130
- WEAK identifier edges: 197
- TRs with at least one cross-TR edge: 162

The JSON snapshot stores one row per TR with transport, module, group, order
flag, rate bucket, `self_fields`, `date_fields`, cross-TR edges, and
environmental dimensions.

## Derivation Rules Preserved

Pure field-name matching produced too much noise in the Migration Source, so
the extracted dataset keeps its identifier-gated rule:

- Order-number aliases form STRONG/NORMAL cross-TR couplings when a consuming
  TR needs an order number produced by a same-domain order/inquiry TR.
- Caller-supplied identifiers such as instrument, market, account, country, and
  currency codes are WEAK couplings. They are useful discovery hints, not hard
  prerequisites.
- `cts_*` fields are SELF continuation fields, not producer/consumer TR edges.
- Environmental dimensions such as market session, paper incompatibility,
  account state, and trading-day sensitivity are facets, not coupling strength.

The snapshot preserves the old derivation's `alias_map` and `weak_seed` so
future maintainers can reconstruct why an edge exists.

## Market / Session Distribution

| Market/session class | TR count |
|---|---:|
| `session_independent` | 115 |
| `market_session_dependent` | 169 |
| `us_market` | 51 |
| `krx_regular` | 4 |
| `krx_night_derivatives` | 11 |
| `event_dependent` | 5 |
| `scenario_manual` | 3 |
| `paper_incompatible` | 7 |

## Paper-Incompatible TRs

These seven TRs were classified as paper-incompatible in the Migration Source
dataset. The runtime rule remains stricter: a live response is
paper-incompatible only when LS returns `01900`.

| TR | Domain | Name |
|---|---|---|
| `CIDBQ01400` | overseas_futures | 해외선물 체결내역개별 조회(주문가능수량) |
| `CIDBQ01500` | overseas_futures | 해외선물 미결제잔고내역 조회 |
| `CIDBQ01800` | overseas_futures | 해외선물 주문내역 조회 |
| `CIDBQ02400` | overseas_futures | 해외선물 주문체결내역 상세 조회 |
| `CIDBQ03000` | overseas_futures | 해외선물 예수금/잔고현황 |
| `CIDBQ05300` | overseas_futures | 해외선물 예탁자산 조회 |
| `CIDEQ00800` | overseas_futures | 일자별 미결제 잔고내역 |

## Manual-Only Order TRs

These are not candidates for ordinary automated evidence. Any maintained order
runtime work must treat them as guarded manual/order-safety surfaces.

| TR | Domain | Name |
|---|---|---|
| `CSPAT00601` | stock | 현물주문 |
| `CSPAT00701` | stock | 현물정정주문 |
| `CSPAT00801` | stock | 현물취소주문 |
| `CFOAT00100` | futures_options | 선물옵션 정상주문 |
| `CFOAT00200` | futures_options | 선물옵션 정정주문 |
| `CFOAT00300` | futures_options | 선물옵션 취소주문 |
| `CFOBQ10800` | futures_options | 선물옵션 옵션매도시 주문증거금조회(옵션매도시 1계약당 주문증거금) |
| `CCENT00100` | futures_options | KRX야간파생 위탁 신규 주문 |
| `CCENT00200` | futures_options | KRX야간파생 위탁 정정 주문 |
| `CCENT00300` | futures_options | KRX야간파생 위탁 취소 주문 |
| `CIDBT00100` | overseas_futures | 해외선물 신규주문 |
| `CIDBT00900` | overseas_futures | 해외선물 정정주문 |
| `CIDBT01000` | overseas_futures | 해외선물 취소주문 |
| `COSAT00301` | overseas_stock | 미국시장주문 API |
| `COSAT00311` | overseas_stock | 미국시장정정주문 API |
| `COSMT00300` | overseas_stock | 해외증권 매도상환주문(미국) |
| `COSAT00400` | overseas_stock | 해외주식 예약주문 등록 및 취소 |

## STRONG Order-Number Consumers

The full JSON contains all 130 STRONG edges. This table lists the consuming TRs
and representative producer samples. Future order expansion should start here
instead of rediscovering order-number flow from raw specs.

| Consuming TR | Domain | Field | Representative producers |
|---|---|---|---|
| `CSPAQ13700` | stock | `SrtOrdNo2` | `CSPAT00601`, `CSPAT00701`, `t0425` |
| `CSPAT00701` | stock | `OrgOrdNo` | `CSPAQ13700`, `CSPAT00601`, `t0425` |
| `CSPAT00801` | stock | `OrgOrdNo` | `CSPAQ13700`, `CSPAT00601`, `t0425` |
| `CCENQ30100` | futures_options | `SrtOrdNo2` | `CFOAQ00600`, `CFOAT00100`, `t0434` |
| `CFOAT00200` | futures_options | `OrgOrdNo` | `CCENQ30100`, `CFOAQ00600`, `t0434` |
| `CFOAT00300` | futures_options | `OrgOrdNo` | `CCENQ30100`, `CFOAQ00600`, `t0434` |
| `CCENT00200` | futures_options | `OrgOrdNo` | `CCENQ30100`, `CFOAQ00600`, `t0434` |
| `CCENT00300` | futures_options | `OrgOrdNo` | `CCENQ30100`, `CFOAQ00600`, `t0434` |
| `CIDBT00900` | overseas_futures | `OvrsFutsOrgOrdNo` | `CIDBQ01800`, `CIDBQ02400`, `CIDBT01000` |
| `CIDBT01000` | overseas_futures | `OvrsFutsOrgOrdNo` | `CIDBQ01800`, `CIDBQ02400`, `CIDBT00900` |
| `COSAQ00102` | overseas_stock | `SrtOrdNo` | `AS0`, `AS1`, `COSAQ01400` |
| `COSAT00301` | overseas_stock | `OrgOrdNo` | `AS0`, `COSAQ00102`, `COSAQ01400` |
| `COSAT00311` | overseas_stock | `OrgOrdNo` | `AS0`, `COSAQ00102`, `COSAQ01400` |
| `COSMT00300` | overseas_stock | `OrgOrdNo` | `AS0`, `COSAQ00102`, `COSAQ01400` |
| `COSAT00400` | overseas_stock | `RsvOrdNo` | `AS0`, `COSAQ00102`, `COSAQ01400` |

## Use In This Repository

Use this snapshot to seed future **SDK Expansion Work Items**. Do not treat it as
the current **Maintained SDK Surface**. For a TR to become maintained, create or
update its `metadata/trs/<tr>.yaml`, add runtime behavior if implemented, select
a Change-Scoped Gate, and make an explicit Focused Evidence decision.

