# 리팩터링 전략 & 아키텍처 적합성 분석

- **작성일**: 2026-07-02
- **작성 모델**: Opus 4.8 (분석 단계)
- **대상**: `korea-adapter-sdk-ls` (LS증권 Open API Rust SDK + metadata/tracking 툴체인)
- **질문 3가지**:
  1. 분석은 Opus 4.8로, 계획/실행은 Fable로 — 이 분리가 맞는가?
  2. "코드가 너무 길다"를 Clean / Hexagonal / Vertical Slice 로 풀어야 하는가?
  3. 이 프로젝트에 그런 패러다임이 애초에 **필요한가**?

---

## 0. TL;DR — 결론 3개

1. **모델 분리는 맞다. 지금처럼 하라.**
   분석은 Opus 4.8(=이 문서)로 끝내고, 계획+실행은 Fable로. 이유는 §6.
   분석은 *짧고 · 고레버리지 · 오류가 하류로 전파*되는 단계라 무거운 추론 모델에 투자하는 게 이득이고, 산출물이 *문서*라서 모델 교체에 따른 컨텍스트 손실이 없다(문서가 곧 공유 컨텍스트). 덤으로 "분석 모델 ≠ 계획 모델"은 이 프로젝트가 이미 애용하는 **교차-모델 검증**을 공짜로 얻는다.

2. **Clean / Hexagonal 을 통째로 도입하는 건 이 프로젝트에서 범주 오류(category error)다.**
   그 패러다임들은 *의존성 방향*과 *비즈니스 로직 격리*를 푸는 도구인데, 이 코드의 "길이"는 그 문제가 아니다. 이건 **라이브러리(SDK)**이고, 비즈니스 로직은 이미 `ls-core`에 얇게 격리돼 있다. 길이의 정체는 §2에서 실증하듯 **320개 TR × 기계적 DTO 보일러플레이트**다. Clean/Hexagonal을 올린다고 이 보일러플레이트는 단 한 줄도 줄지 않는다 — 오히려 ceremony만 늘어난다.

3. **진짜 레버는 "메타데이터 기반 코드 생성(codegen)"이다.**
   이 프로젝트는 이미 320개 TR의 wire-shape를 **구조화된 형태로 보유**하고 있다(`normalized/trs/*.json` + `metadata/trs/*.yaml`). 이미 그걸로 **문서를 생성**한다(`ls-docgen`). DTO 스켈레톤을 생성하는 건 그 자연스러운 확장이며, 이 프로젝트에만 열려 있는 고유한 수단이다. 우선순위: **codegen > 선언적 매크로 > 모듈 분해 계속 > vertical-slice 형식화**.

---

## 1. 현재 코드베이스 실측 (2026-07-02 측정)

Cargo workspace, `resolver = "2"`, 6개 크레이트.

| 크레이트 | LOC | 파일 수 | 역할 |
|---|---:|---:|---|
| **`ls-sdk`** | **61,362** | **100** | 공개 SDK: TR별 request/response 구조체 + facade 핸들 |
| `ls-trackers` | 13,189 | 13 | API-drift / spec 트래커 (툴링) |
| `ls-core` | 12,100 | 24 | 런타임: dispatch, endpoint policy, rate limiter, auth, preflight |
| `ls-metadata` | 2,955 | 8 | 메타데이터 스키마 + validator |
| `ls-docgen` | 1,896 | 3 | 메타데이터 → docs/ 투영 |
| `ls-sdk-test-support` | 464 | 4 | wiremock 오프라인 테스트 헬퍼 |

**핵심 수치**
- TR 개수: **320** (`metadata/trs/*.yaml` 320 = `normalized/trs/*.json` 320, 1:1 대응)
- `ls-sdk/src` 내 `pub struct`: **210개**, serde 속성(`serialize_with`/`deserialize_with`/`serde(...)`): **211개** — 구조체당 거의 1개꼴로 wire-type 관용 처리
- 가장 큰 소스 파일들:
  - `ls-trackers/src/cli.rs` 3,374 · `ls-trackers/src/api_drift.rs` 2,451 (둘 다 툴링)
  - `ls-sdk/src/paginated/chart.rs` 2,304 · `market_session/investor_flow.rs` 1,775 · `masters.rs` 1,660 · `elw.rs` 1,617 · `charts.rs` 1,580
  - `ls-core/src/inner.rs` 1,792 (dispatch 코어)
  - `ls-sdk/src/realtime/frame.rs` 1,471

**이미 되어 있는 구조 분해** (메모리/PR 이력 기준):
- `market_session/mod.rs` 11,789 → 1,157 로 분해 (PR #72)
- `frame.rs` 4,977 → 1,471, `account` 2,406 → 316, `endpoint_policy` 5,077 → 396 + 6 서브모듈
- 테스트 파일도 per-family 분해 완료 (PR #78)
- 분해 기법: `pub use submod::*;` (순수 relocation) → count 테스트·crosscheck 리스트를 **한 줄도 안 건드리고** 유지

> 즉, "파일이 길다"는 축은 이미 상당히 손을 봤다. 남은 건 파일 개수가 아니라 **줄 수의 총량**이고, 그 총량은 `ls-sdk`에 몰려 있다.

---

## 2. "길이" 문제의 진짜 정체

`ls-sdk`가 61k LOC인 이유는 **아키텍처 얽힘이 아니라 형식적 DTO의 물량**이다. 증거물(exhibit)로 `t8425`(전체테마, 가장 단순한 read) 하나를 보자:

```rust
// InBlock — 요청 필드 목록
#[derive(Serialize, Debug, Clone)]
pub struct T8425InBlock { pub dummy: String }

// Request — InBlock을 "t8425InBlock" 키로 감싸는 봉투
#[derive(Serialize, Debug, Clone)]
pub struct T8425Request {
    #[serde(rename = "t8425InBlock")] pub inblock: T8425InBlock,
}
impl T8425Request { pub fn new() -> Self { /* ... */ } }
impl Default for T8425Request { /* ... */ }

// OutBlock — 응답 한 행
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct T8425OutBlock {
    #[serde(deserialize_with = "ls_core::string_or_number")] pub tmname: String,
    #[serde(deserialize_with = "ls_core::string_or_number")] pub tmcode: String,
}

// Response — 봉투 + rsp_cd/rsp_msg + 배열-혹은-단일 관용
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct T8425Response {
    #[serde(default)] pub rsp_cd: String,
    #[serde(default)] pub rsp_msg: String,
    #[serde(rename = "t8425OutBlock", default, deserialize_with = "ls_core::de_vec_or_single")]
    pub outblock: Vec<T8425OutBlock>,
}
```

이 ~90줄(doc 주석 포함) 중 **구조적으로 결정되는 부분**:

- `InBlock`/`OutBlock`의 필드명·타입·length·required → **이미 baseline JSON에 있다**:
  ```json
  { "direction":"request", "block_name":"...", "field_name":"tr_cd",
    "korean_name":"거래 CD", "type":"String", "length":10, "required":true }
  ```
- `Request` 봉투(`rename = "{tr}InBlock"`), `new()`, `Default`, `Response`의 `rsp_cd`/`rsp_msg` + `de_vec_or_single` 관용 → **모든 TR이 동일 패턴**
- 배열-vs-단일 여부, self-pagination 여부, rate bucket, protocol(rest/ws) → **`metadata/trs/*.yaml`의 facets에 이미 있다**

**결론**: 이 320벌 × ~90줄의 대부분은 *프로젝트가 이미 구조화해서 들고 있는 데이터의 손번역본*이다. 사람이 새로 판단해서 쓰는 부분은 (a) doc 주석 산문, (b) baseline이 request 블록을 과소보고할 때의 certified override(§7 리스크) 정도다.

이게 왜 중요한가: **길이 문제의 해법은 "구조를 바꾸는 것"이 아니라 "물량을 생성으로 대체하는 것"**이다. 아키텍처 패러다임은 물량 문제를 못 푼다.

---

## 3. 현재 아키텍처 평가 — 이미 꽤 좋다

실제 구조를 그려보면:

```
                    ┌─────────────────────────────────────────┐
   호출자 (앱)  ──▶ │  ls-sdk  (Driving Adapter / 공개 API)    │
                    │  LsSdk facade → 도메인별 핸들:            │
                    │   standalone · market_session · paginated │
                    │   account · orders · fo_orders · realtime │
                    └───────────────┬─────────────────────────┘
                                    │  Arc<Inner> (공유 런타임)
                    ┌───────────────▼─────────────────────────┐
                    │  ls-core  (Application Core / 런타임)     │
                    │   Inner::post / post_paginated / post_order│
                    │   endpoint_policy/{market_data,order,      │
                    │     realtime_md,realtime_order,auth,...}   │
                    │   rate_limiter · auth · preflight · error  │
                    └───────────────┬─────────────────────────┘
                                    │  HTTP / WebSocket
                              LS 게이트웨이 (외부)

   ┌─────────────── 메타데이터/툴링 평면 (Source of Truth) ───────────────┐
   │ metadata/trs/*.yaml + normalized/trs/*.json  →  ls-docgen  →  docs/  │
   │ ls-metadata (validator) · ls-trackers (drift/freshness)             │
   └─────────────────────────────────────────────────────────────────────┘
```

여기서 읽어낼 사실:

- **관심사 분리(Separation of Concerns)는 이미 크레이트 경계로 되어 있다.** transport/dispatch(`ls-core`) ↔ 공개 API(`ls-sdk`) ↔ 메타데이터/툴링(metadata/trackers/docgen). 서로 다른 축의 변경이 서로를 오염시키지 않는다.
- **Hexagonal(ports & adapters)의 골격이 이미 있다.** `ls-core`가 도메인-무관 코어(hexagon), `endpoint_policy/*`가 도메인별 정책 포트, `ls-sdk`의 facade 핸들이 driving adapter. 게이트웨이는 driven side.
- **Vertical Slice도 이미 사실상 적용돼 있다.** `market_session / paginated / account / orders / realtime`는 "레이어"가 아니라 **의존성-클래스(feature) 슬라이스**다. `lib.rs` 주석도 명시한다: *"Dependency classes are modules within this crate ... The Change-Scoped Gate routes tests by metadata facet, not by module boundary."* — 이건 vertical slice 철학 그 자체다.

> 즉, 사용자가 "도입할까?" 고민 중인 세 패러다임 중 둘(Hexagonal의 코어/어댑터 분리, Vertical Slice)은 **이미 이 코드에 내재**해 있다. 남은 건 "도입"이 아니라 "형식화/일관화"다.

---

## 4. 패러다임 적합성 판정

| 패러다임 | 무엇을 푸는가 | 이 프로젝트에 필요한가 | 판정 |
|---|---|---|---|
| **Clean Architecture** | 엔티티/유스케이스/인터페이스를 동심원으로, 의존성이 안쪽으로만 향하게 | 이 SDK엔 "유스케이스/엔티티" 계층이 없다. 로직은 dispatch 정책뿐이고 이미 `ls-core`에 격리됨. UseCase/Interactor/Entity 레이어를 올리면 320개 TR마다 빈 껍데기 계층이 늘어남 | ❌ **도입 금지.** 물량 문제 미해결 + ceremony 폭증 |
| **Hexagonal (Ports & Adapters)** | 코어 로직을 I/O(어댑터)와 분리, 어댑터 교체 가능 | 골격이 이미 있음(`ls-core`=코어, facade=어댑터). 굳이 "정식 hexagonal"로 리네이밍/추상화할 실익 적음 | 🟡 **이미 있음. 추가 도입 불필요.** 원한다면 용어/경계만 문서화 |
| **Vertical Slice** | 레이어가 아니라 기능(슬라이스) 단위로 코드 조직 | 이미 dependency-class 모듈 = 슬라이스. 테스트도 facet 기반 라우팅 | 🟢 **이미 채택됨. "완성/형식화"만.** 신규 TR이 항상 올바른 슬라이스에 떨어지도록 규칙화 |
| **메타데이터 기반 Codegen** | 구조화된 명세로부터 반복 코드를 생성 | 320 TR × 기계적 DTO = 정확히 이 문제. 명세(baseline+yaml)와 생성 인프라(docgen) 이미 보유 | ✅ **이게 정답.** 유일하게 "길이"를 실제로 공격함 |

**한 문장 요약**: 사용자가 후보로 든 세 패러다임은 *앱 아키텍처* 도구인데, 이 프로젝트의 통증은 *라이브러리의 DTO 물량*이다. 세 패러다임 중 유효한 부분은 이미 코드에 있고, 진짜 해법(codegen)은 후보 목록에 없었다.

---

## 5. 권장 리팩터링 레버 (우선순위순)

### 레버 1 — 메타데이터 기반 DTO codegen  ⭐ 최고 레버리지
- **무엇**: `normalized/trs/*.json`(필드-레벨 wire shape) + `metadata/trs/*.yaml`(facets)로부터 `InBlock/Request/new/Default/OutBlock/Response` 스켈레톤을 생성. `ls-docgen`을 확장하거나 형제 `ls-sdk-gen` 크레이트로.
- **왜 이 프로젝트에서만 가능**: source of truth가 이미 구조화돼 있고, `make docs` / `docs-check`로 "생성물 == 커밋된 것" 검증 파이프라인도 이미 있다. codegen을 같은 파이프라인에 태우면 된다.
- **효과**: (a) 신규 TR 손코딩 비용 ≈ 0, (b) wire drift 자동 전파, (c) 기계적 부분 일회성 대폭 축소.
- **⚠️ 하이브리드로 해야 함 (프로젝트의 기존 교훈 반영)**:
  - **Response/OutBlock**: baseline이 응답 shape의 권위 → 자신 있게 생성.
  - **Request/InBlock**: *"baseline은 request 블록을 과소보고할 수 있고, live-certified SDK request 구조체가 있으면 그게 이긴다"*(`docs/solutions/conventions/normalized-baseline-can-underreport-request-block.md`). → 생성물은 **시작 스켈레톤**이고, 손으로 certified한 구조체가 override 가능해야 함. 눈 감고 전량 생성 금지.
  - **doc 주석**: 사람이 쓴 산문(T8425 예시의 질 높은 주석)은 생성 불가 → 사실 기반 stub만 생성, 필요 시 사람이 보강.
  - 숫자 request 필드는 `string_as_number`(JSON 숫자)여야 `IGW40011` 안 남 — codegen이 이 규칙을 facet(type=Long/Number)에서 자동 적용해야 함.
- **정직한 기대치**: "61k 줄 삭제"가 아니라 **성장 멈춤 + 기계적 부분 축소**. doc-heavy 파일은 0으로 안 줄어든다.
- **비용/리스크**: 中. 생성 인프라 1회 구축 + 320개에 대한 생성물이 gate를 통과하도록 안정적 출력 보장.

### 레버 2 — 선언적 매크로 `tr!` (codegen의 경량 대안/디딤돌)
- **무엇**: Request 봉투 + `new` + `Default` + Response(`rsp_cd`/`rsp_msg` + `de_vec_or_single`)의 동일 패턴을 `macro_rules!`로. (이미 `rank_screen.rs`에 `rank_row!`/`idx_summary!` 선례 있음.)
- **효과**: 각 TR의 ~40%(봉투/impl 반복)를 흡수. baseline override 문제를 안 건드림(필드 목록은 여전히 손).
- **비용/리스크**: 低. codegen이 부담되면 여기부터. 나중에 codegen으로 승격 가능.

### 레버 3 — 모듈 분해 계속 (이미 검증된 패턴)
- 남은 대형 파일(`chart.rs` 2,304, `investor_flow.rs` 1,775 등)을 `pub use submod::*;` 순수 relocation으로 계속 쪼갬. count 테스트·crosscheck 무변경.
- **비용/리스크**: 低. 단, relocation은 *실패가 조용함* → 검증은 컴파일러가 아니라 `--list` 테스트명 스냅샷 diff (PR #78 교훈).

### 레버 4 — Vertical slice 형식화 (구조 아님, 규칙)
- 신규 TR이 항상 owner_class(facet) → 올바른 모듈 슬라이스로 떨어지도록 `track-tr`/`implement-tr` 레시피에 명문화. 이미 사실상 지켜지고 있으니 "규칙 고정"만.

**하지 말 것**
- Clean/Hexagonal 대공사(빅뱅 재설계). 물량 미해결 + gate 결합(§7) 대규모 파손 위험.
- 메타데이터 source-of-truth나 test crosscheck 리스트와 싸우는 어떤 변경도.

---

## 6. 모델 선택 전략 — 왜 "Opus 분석 / Fable 계획·실행"인가

**결정 규칙**: *짧고 고레버리지인 사고는 가장 무거운 추론 모델에, 길고 기계적인 실행은 가장 효율적인 최신 모델에. 그리고 각 단계를 상대 모델로 교차 검증.*

이 프로젝트에 적용하면:

| 단계 | 성격 | 권장 모델 | 이유 |
|---|---|---|---|
| **분석 (지금)** | 짧음 · 고레버리지 · 오류가 전 하류로 전파 | **Opus 4.8** | 방향을 정하는 단계. 여기서 틀리면 계획·실행이 통째로 틀림. 산출물이 *문서*라 모델 교체 시 컨텍스트 손실 0 (문서 = 공유 컨텍스트) |
| **계획** | 다수 모듈에 걸친 설계 | **Fable** | 물량 작업. + 분석(Opus)과 다른 모델이라 **교차-모델 검증**이 공짜로 붙음 |
| **실행** | 100개 파일 편집·생성 반복 | **Fable** | 처리량이 곧 비용. 최신·효율 모델의 throughput이 여기서 복리로 이득 |
| **리뷰** | 계획/코드 검증 | 상대 모델 (Fable 계획 → Opus 리뷰, 또는 그 반대) | 이 레포는 이미 "cross-model Codex 리뷰"를 상시 사용(메모리 다수 기록). 관성과 일치 |

**"전부 Fable로" 대안은 언제 맞나**
- 단일 모델의 일관성(one voice)과 낮은 비용을 교차검증보다 더 치는 경우.
- 실사용에서 Fable의 아키텍처 추론이 Opus와 대등하다고 느끼는 경우.
- 이 경우 손실은 "교차-모델 다양성"뿐. 산출물이 문서라 컨텍스트 연속성은 어차피 문제 안 됨.

**추천**: 분석은 이미 Opus로 하고 있으니(=이 문서) 그대로 확정. 계획·실행은 Fable. 그리고 **Fable이 낸 계획을 Opus로 1회 리뷰**(§8 리스크 목록을 체크리스트로). 짧은 고가치 사고엔 비싼 모델, 긴 기계 작업엔 효율 모델 — 딱 이 프로젝트의 phase 구조와 맞아떨어진다.

---

## 7. 리스크 & 제약 (계획 모델에 반드시 전달)

이 레포는 gate 머신러리가 현재 구조에 **강하게 결합**돼 있다. 리팩터링이 이걸 존중하지 않으면 green gate가 대량 파손된다.

1. **Count 리터럴 결합**: `ls-docgen/src/lib.rs`의 `reference.len()` 값(현 280)·`banner_trs` allowlist, `ls-trackers/src/cli.rs`의 여러 count 리터럴은 `make docs`로 안 잡히고 **`cargo test`로만** 잡힌다. codegen/분해가 이 수치를 흔들면 손으로 맞춰야 함.
2. **Crosscheck 리스트 이원화**: 신규 REST `{TR}_POLICY`는 **두 crosscheck 리스트 모두**에 등록. WebSocket policy(`owner_class: realtime`)는 crosscheck 리스트에만, `slice_rest_policies_are_non_order_rest`엔 절대 안 넣음.
3. **순수 relocation 트릭**: 모듈 분해는 `pub use submod::*;`로 공개 경로 불변 유지. `use super::*`가 부모의 *private* import까지 glob으로 끌어와서 순수 이동이 컴파일됨 — codegen 출력도 이 관례를 따라야 함.
4. **Baseline 과소보고**: request 블록에서 baseline < 실제일 수 있음. certified 손작성 구조체 우선(§5 레버1 하이브리드).
5. **숫자 request 필드**: `string_as_number` 안 쓰면 게이트웨이 `IGW40011`. F/O 가격은 소수 → `string_as_decimal`(i64 아님).
6. **`ls-trackers` 포맷 금지**: `main`이 의도적으로 unformatted. 전체 `cargo fmt` 하면 거대한 헛디프.
7. **분해 실패는 조용함**: 검증은 컴파일이 아니라 `--list` 테스트명 스냅샷 diff. 한국어 문자열 있는 테스트는 locale 트랩(`LC_ALL=C` 고정).
8. **MEMORY.md 초과**: 현재 29.3KB > 24.4KB 한도. 리팩터링과 무관하지만, 이 작업 기록을 memory에 넣으려면 index 한 줄로만.

---

## 8. 추천 실행 순서 (phased, Fable 담당)

- **Phase 0 (설계, Opus 리뷰 대상)**: codegen 스코프 확정 — Response 전량 생성 / Request 스켈레톤+override 경계 / doc stub 정책. gate 결합(§7) 대응 계획.
- **Phase 1 (PoC)**: 대표 TR 3~5개(단순 read t8425 · paginated · 배열형 · request 있는 것 · order 계열)에 codegen 적용, `docs-check` 방식의 "생성물==커밋" 검증 붙이기. gate green 유지 증명.
- **Phase 2 (확대)**: read 계열 TR로 확대. 파일별로 순수 relocation과 병행.
- **Phase 3 (경계 케이스)**: certified override가 필요한 request/order 계열 신중히. baseline 과소보고 대조.
- **Phase 4 (형식화)**: 신규 TR이 codegen+올바른 슬라이스로 떨어지도록 `track-tr`/`implement-tr` 레시피 갱신.

각 Phase는 독립 PR + `cargo test`/`docs-check`/`lane-check` green + ce-code-review로 마감(레포 관례).

---

## 9. Fable에게 넘길 리팩터링 계획 프롬프트

> 별도 파일로도 저장: `docs/brainstorms/2026-07-02-refactoring-planning-prompt.md`
> `ce-plan` 또는 `ce-brainstorm` 스킬과 함께 Fable 세션에 붙여 쓰면 됨.

```
[역할] 너는 korea-adapter-sdk-ls (LS증권 Open API Rust SDK) 리팩터링 계획을
수립하는 시니어 Rust 아키텍트다. 실행이 아니라 "구현 가능한 계획"을 만든다.

[확정된 방향 — 재논의 금지]
- 이건 라이브러리(SDK)다. Clean/Hexagonal 통짜 도입은 하지 않는다(범주 오류).
  Hexagonal 코어/어댑터 분리와 Vertical Slice는 이미 코드에 내재 → "형식화"만.
- "길이" 문제의 정체는 320개 TR × 기계적 DTO 보일러플레이트다(아키텍처 얽힘 아님).
- 채택 레버 우선순위: (1) 메타데이터 기반 DTO codegen  (2) 선언적 tr! 매크로
  (3) 모듈 분해 계속(pub use submod::*)  (4) vertical-slice 규칙 형식화.
  근거 전체는 docs/brainstorms/2026-07-02-refactoring-strategy-and-architecture-analysis.md 참조.

[목표] 위 레버 (1)을 중심으로 단계별(PoC→확대→경계→형식화) 구현 계획을 작성.
각 단계는 독립 PR 단위, gate green 유지, 롤백 가능해야 한다.

[반드시 반영할 제약 (틀리면 gate 파손)]
1. codegen 소스: crates/ls-trackers/baselines/api-drift/normalized/trs/<tr>.json
   (필드-레벨 wire shape) + metadata/trs/<tr>.yaml (facets). 이미 ls-docgen이
   이 소스로 문서를 생성 중 → 같은 파이프라인/검증(make docs, docs-check)에 태울 것.
2. 하이브리드 생성:
   - Response/OutBlock: baseline이 권위 → 전량 생성.
   - Request/InBlock: baseline이 과소보고 가능. live-certified 손작성 구조체가
     override하면 그게 이긴다(docs/solutions/.../normalized-baseline-can-underreport-request-block.md).
     생성물은 시작 스켈레톤. 눈 감고 전량 생성 금지.
   - doc 주석 산문은 생성 불가 → 사실 기반 stub만.
   - 숫자 request 필드 = string_as_number(JSON 숫자, 아니면 IGW40011).
     F/O 가격 소수 = string_as_decimal(i64 아님).
3. Count/crosscheck 결합: ls-docgen/src/lib.rs의 reference.len()·banner_trs와
   ls-trackers/src/cli.rs count 리터럴은 make docs로 안 잡히고 cargo test로만 잡힘.
   신규 REST {TR}_POLICY는 두 crosscheck 리스트 모두에, WS policy는 crosscheck에만.
4. 순수 relocation은 pub use submod::*; + use super::*; 관례 유지(공개 경로 불변).
   분해 검증은 컴파일이 아니라 cargo test --list 테스트명 스냅샷 diff. 한국어 문자열은
   LC_ALL=C 고정.
5. ls-trackers 크레이트는 cargo fmt 금지(main이 의도적으로 unformatted).

[산출물 형식]
- 단계별 계획(Phase 0~4). 각 Phase: 목표 / 변경 파일 / 검증 방법 / 롤백 / 예상 리스크.
- Phase 1은 대표 TR 3~5개(단순 read·paginated·배열형·request 있는 것·order 계열)로
  PoC. "생성물 == 커밋된 것" 검증을 docs-check 방식으로 붙이는 구체안 포함.
- gate: make docs / cargo test / cargo test -p ls-core / make docs-check / make lane-check.
- 정직한 기대치: "61k줄 삭제"가 아니라 "성장 멈춤 + 기계적 부분 축소"로 프레이밍.

[먼저 할 일] 계획 전에 다음을 실제로 확인하라(추측 금지):
- ls-docgen/src/lib.rs가 baseline/metadata를 읽어 문서를 만드는 방식(codegen 재사용점)
- normalized/trs/*.json의 request/response 블록 스키마 전체 필드
- 대표 TR 파일 3~5개의 실제 구조체 패턴과 doc 주석 밀도
- .agents/skills/track-tr, implement-tr 레시피(신규 TR이 만들어지는 경로)

계획이 서면 docs/plans/2026-07-02-XXX-refactor-metadata-codegen-plan.md 로 저장.
```

---

## 부록 — 이 분석이 근거로 삼은 실측 명령

```bash
# 크레이트별 LOC / 파일 수
for d in crates/*/; do n=$(find "$d" -name '*.rs' | xargs cat | wc -l); \
  f=$(find "$d" -name '*.rs' | wc -l); echo "$n / $f  $d"; done | sort -rn
# TR·baseline·구조체·serde 물량
ls metadata/trs/*.yaml | wc -l            # 320
ls crates/ls-trackers/baselines/api-drift/normalized/trs/*.json | wc -l  # 320
grep -rn "pub struct" crates/ls-sdk/src | wc -l     # 210
grep -rn "serialize_with\|deserialize_with\|serde(" crates/ls-sdk/src | wc -l  # 211
# 증거물
cat crates/ls-sdk/src/market_session/masters.rs      # T8425* 패턴
cat crates/ls-trackers/baselines/api-drift/normalized/trs/t8425.json  # 필드 shape
cat metadata/trs/t8425.yaml                          # facets
```
