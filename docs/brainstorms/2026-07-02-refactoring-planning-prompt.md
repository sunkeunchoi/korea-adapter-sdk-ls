# 리팩터링 계획 프롬프트 (Fable 세션용)

- **작성일**: 2026-07-02
- **용도**: Fable 모델 + `ce-plan`(또는 `ce-brainstorm`) 스킬로 리팩터링 계획을 수립할 때 붙여 쓰는 프롬프트.
- **근거 문서**: `docs/brainstorms/2026-07-02-refactoring-strategy-and-architecture-analysis.md` (Opus 4.8 분석)

> 아래 코드블록 전체를 Fable 세션에 그대로 붙여넣으면 된다.

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

## 프롬프트 변형 (선택)

**변형 A — 레버 (2) tr! 매크로부터 (codegen이 부담스러우면 디딤돌):**
위 프롬프트에서 `[목표]`를 다음으로 교체 —
```
[목표] 레버 (2) 선언적 tr! 매크로부터 시작한다. Request 봉투 + new + Default +
Response(rsp_cd/rsp_msg + de_vec_or_single) 동일 패턴을 macro_rules!로 흡수.
선례: crates/ls-sdk/src/paginated/rank_screen.rs의 rank_row!/idx_summary!.
매크로 적용 전후로 cargo test --list 스냅샷이 불변임을 증명하는 계획을 포함.
이후 codegen(레버 1)으로 승격하는 경로도 밑그림으로 남길 것.
```

**변형 B — Fable 계획을 Opus로 교차 리뷰:**
Fable가 계획을 낸 뒤 Opus 4.8 세션에서 —
```
아래 리팩터링 계획을 docs/brainstorms/2026-07-02-refactoring-strategy-and-architecture-analysis.md
§7 리스크 목록을 체크리스트로 삼아 검증하라. 각 리스크 항목에 대해 계획이
(a) 다루는지 (b) 어떻게 다루는지 (c) 빠졌다면 무엇을 추가해야 하는지 판정.
특히 count/crosscheck 결합과 baseline 과소보고 override 경계를 집중 검증.
```
