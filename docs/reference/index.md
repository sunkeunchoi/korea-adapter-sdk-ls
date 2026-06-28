# SDK Reference Docs

> Generated from `ls-metadata` — do not edit by hand. Run `make docs` to regenerate.

Minimal user-facing reference for the implemented TRs. Tracked-but-unimplemented TRs are intentionally excluded; see the TR Dependency Docs for the full tracked set.

| TR | Name | Owner class | Status |
|----|------|-------------|--------|
| `AS0` | 해외주식 주문접수 실시간 | `realtime` | implemented, not yet recommended |
| `AS1` | 해외주식 주문체결 실시간 | `realtime` | implemented, not yet recommended |
| `AS2` | 해외주식 주문정정 실시간 | `realtime` | implemented, not yet recommended |
| `AS3` | 해외주식 주문취소 실시간 | `realtime` | implemented, not yet recommended |
| `AS4` | 해외주식 주문거부 실시간 | `realtime` | implemented, not yet recommended |
| `C01` | 선물옵션 주문체결 실시간 | `realtime` | implemented, not yet recommended |
| `CFOAQ10100` | 선물옵션 주문가능수량조회 | `account` | implemented, not yet recommended |
| `CFOBQ10500` | 선물옵션 계좌예탁금증거금조회 | `account` | implemented, not yet recommended |
| `CFOEQ11100` | 선물옵션가정산예탁금상세 | `account` | implemented, not yet recommended |
| `CIDBQ01400` | 해외선물 체결내역개별 조회(주문가능수량) | `account` | implemented, not yet recommended |
| `CIDBQ03000` | 해외선물 예수금/잔고현황 | `account` | implemented, not yet recommended |
| `CIDBQ05300` | 해외선물 예탁자산 조회 | `account` | implemented, not yet recommended |
| `CLNAQ00100` | 예탁담보융자가능종목현황조회 | `account` | implemented, not yet recommended |
| `CSPAQ12200` | 현물계좌 예수금/주문가능금액/총평가 조회 | `account` | recommended |
| `CSPAQ12300` | BEP단가조회 | `account` | implemented, not yet recommended |
| `CSPAQ22200` | 현물계좌예수금 주문가능금액 총평가2 | `account` | implemented, not yet recommended |
| `CSPAT00601` | 현물 정규주문 (cash equity order submission) | `orders` | implemented, not yet recommended |
| `CSPAT00701` | 현물정정주문 (cash equity order modify) | `orders` | implemented, not yet recommended |
| `CSPAT00801` | 현물취소주문 (cash equity order cancel) | `orders` | implemented, not yet recommended |
| `FC9` | 선물 체결 실시간 시세 | `realtime` | implemented, not yet recommended |
| `FH9` | 선물 호가 실시간 시세 | `realtime` | implemented, not yet recommended |
| `GSC` | 해외주식 체결 실시간 시세 | `realtime` | implemented, not yet recommended |
| `GSH` | 해외주식 호가 실시간 시세 | `realtime` | implemented, not yet recommended |
| `H01` | 선물옵션 주문정정취소 실시간 | `realtime` | implemented, not yet recommended |
| `H1_` | KOSPI 호가 실시간 시세 | `realtime` | implemented, not yet recommended |
| `HA_` | KOSDAQ 호가 실시간 시세 | `realtime` | implemented, not yet recommended |
| `K3_` | KOSDAQ 체결 실시간 시세 | `realtime` | implemented, not yet recommended |
| `O01` | 선물옵션 주문접수 실시간 | `realtime` | implemented, not yet recommended |
| `OC0` | 옵션 체결 실시간 시세 | `realtime` | implemented, not yet recommended |
| `OH0` | 옵션 호가 실시간 시세 | `realtime` | implemented, not yet recommended |
| `OVC` | 해외선물 체결 실시간 시세 | `realtime` | implemented, not yet recommended |
| `OVH` | 해외선물 호가 실시간 시세 | `realtime` | implemented, not yet recommended |
| `S2_` | KOSPI 우선호가 실시간 시세 | `realtime` | implemented, not yet recommended |
| `S3_` | KOSPI 체결 실시간 시세 | `realtime` | recommended |
| `SC0` | 주식 주문접수 실시간 | `realtime` | implemented, not yet recommended |
| `SC1` | 주식 주문체결 실시간 | `realtime` | implemented, not yet recommended |
| `SC2` | 주식 주문정정 실시간 | `realtime` | implemented, not yet recommended |
| `SC3` | 주식 주문취소 실시간 | `realtime` | implemented, not yet recommended |
| `SC4` | 주식 주문거부 실시간 | `realtime` | implemented, not yet recommended |
| `TC1` | 해외선물 주문접수 실시간 | `realtime` | implemented, not yet recommended |
| `TC2` | 해외선물 주문응답 실시간 | `realtime` | implemented, not yet recommended |
| `TC3` | 해외선물 주문체결 실시간 | `realtime` | implemented, not yet recommended |
| `UH1` | 통합 호가 실시간 시세 | `realtime` | implemented, not yet recommended |
| `US2` | 통합 우선호가 실시간 시세 | `realtime` | implemented, not yet recommended |
| `US3` | 통합 체결 실시간 시세 | `realtime` | implemented, not yet recommended |
| `o3101` | 해외선물마스터조회 | `market_session` | implemented, not yet recommended |
| `o3105` | 해외선물 현재가(종목정보) 조회 | `market_session` | implemented, not yet recommended |
| `o3106` | 해외선물 현재가호가 조회 | `market_session` | implemented, not yet recommended |
| `o3121` | 해외선물옵션 마스터 조회 | `market_session` | implemented, not yet recommended |
| `o3125` | 해외선물옵션 현재가(종목정보) 조회 | `market_session` | implemented, not yet recommended |
| `o3126` | 해외선물옵션 현재가호가 조회 | `market_session` | implemented, not yet recommended |
| `revoke` | 접근토큰 폐기 (OAuth2 token revoke) | `standalone` | implemented, not yet recommended |
| `t0167` | 서버시간조회 | `market_session` | implemented, not yet recommended |
| `t0424` | 주식잔고2 | `account` | implemented, not yet recommended |
| `t0425` | 주식체결/미체결 (stock filled/unfilled order inquiry) | `paginated` | implemented, not yet recommended |
| `t1101` | 주식 현재가호가 조회 | `market_session` | recommended |
| `t1102` | 주식 현재가(시세) 조회 | `market_session` | recommended |
| `t1104` | 주식현재가시세메모 | `market_session` | implemented, not yet recommended |
| `t1105` | 주식피봇/디마크조회 | `market_session` | implemented, not yet recommended |
| `t1302` | 주식분별주가조회 | `market_session` | implemented, not yet recommended |
| `t1305` | 기간별주가 | `paginated` | implemented, not yet recommended |
| `t1308` | 주식시간대별체결조회챠트 | `market_session` | implemented, not yet recommended |
| `t1310` | 주식당일전일분틱조회 | `paginated` | implemented, not yet recommended |
| `t1403` | 신규상장종목조회 | `paginated` | implemented, not yet recommended |
| `t1404` | 관리/불성실/투자유의조회 | `paginated` | implemented, not yet recommended |
| `t1405` | 투자경고/매매정지/정리매매조회 | `paginated` | implemented, not yet recommended |
| `t1410` | 초저유동성조회 | `paginated` | implemented, not yet recommended |
| `t1411` | 증거금율별종목조회 | `paginated` | implemented, not yet recommended |
| `t1422` | 상/하한 | `paginated` | implemented, not yet recommended |
| `t1427` | 상/하한가직전 | `paginated` | implemented, not yet recommended |
| `t1441` | 등락율상위 | `paginated` | implemented, not yet recommended |
| `t1442` | 신고/신저가 | `paginated` | implemented, not yet recommended |
| `t1444` | 시가총액상위 | `paginated` | implemented, not yet recommended |
| `t1449` | 가격대별매매비중조회 | `market_session` | implemented, not yet recommended |
| `t1452` | 거래량상위 | `paginated` | implemented, not yet recommended |
| `t1463` | 거래대금상위 | `paginated` | implemented, not yet recommended |
| `t1466` | 전일동시간대비거래급증 | `paginated` | implemented, not yet recommended |
| `t1481` | 시간외등락율상위 | `paginated` | implemented, not yet recommended |
| `t1482` | 시간외거래량상위 | `paginated` | implemented, not yet recommended |
| `t1485` | 예상지수 | `market_session` | implemented, not yet recommended |
| `t1488` | 예상체결가등락율상위조회 | `paginated` | implemented, not yet recommended |
| `t1489` | 예상체결량상위조회 | `paginated` | implemented, not yet recommended |
| `t1492` | 단일가예상등락율상위 | `paginated` | implemented, not yet recommended |
| `t1511` | 업종현재가 | `market_session` | implemented, not yet recommended |
| `t1514` | 업종기간별추이 | `paginated` | implemented, not yet recommended |
| `t1516` | 업종별종목시세 | `market_session` | implemented, not yet recommended |
| `t1531` | 테마별종목 | `market_session` | implemented, not yet recommended |
| `t1532` | 종목별테마 | `market_session` | implemented, not yet recommended |
| `t1533` | 특이테마 | `market_session` | implemented, not yet recommended |
| `t1537` | 테마종목별시세조회 | `market_session` | implemented, not yet recommended |
| `t1601` | 투자자별종합 | `market_session` | implemented, not yet recommended |
| `t1615` | 투자자매매종합1 | `market_session` | implemented, not yet recommended |
| `t1621` | 업종별분별투자자매매동향(챠트용) | `market_session` | implemented, not yet recommended |
| `t1636` | 종목별프로그램매매동향 | `paginated` | implemented, not yet recommended |
| `t1638` | 종목별잔량/사전공시 | `market_session` | implemented, not yet recommended |
| `t1640` | 프로그램매매종합조회(미니) | `market_session` | implemented, not yet recommended |
| `t1662` | 시간대별프로그램매매추이(차트) | `market_session` | implemented, not yet recommended |
| `t1664` | 투자자매매종합(챠트) | `market_session` | implemented, not yet recommended |
| `t1764` | 회원사리스트 | `market_session` | implemented, not yet recommended |
| `t1809` | 신호조회 | `paginated` | implemented, not yet recommended |
| `t1825` | 종목Q클릭검색(씽큐스마트) | `market_session` | implemented, not yet recommended |
| `t1826` | 종목Q클릭검색리스트조회(씽큐스마트) | `market_session` | implemented, not yet recommended |
| `t1859` | 서버저장조건 조건검색 | `market_session` | implemented, not yet recommended |
| `t1866` | 서버저장조건 리스트조회 | `paginated` | implemented, not yet recommended |
| `t1901` | ETF현재가(시세)조회 | `market_session` | implemented, not yet recommended |
| `t1903` | ETF일별추이 | `market_session` | implemented, not yet recommended |
| `t1906` | ETFLP호가 | `market_session` | implemented, not yet recommended |
| `t1921` | 신용거래동향 | `paginated` | implemented, not yet recommended |
| `t1926` | 종목별신용정보 | `market_session` | implemented, not yet recommended |
| `t1950` | ELW현재가(시세)조회 | `market_session` | implemented, not yet recommended |
| `t1956` | ELW현재가(확정지급액)조회 | `market_session` | implemented, not yet recommended |
| `t1958` | ELW종목비교 | `market_session` | implemented, not yet recommended |
| `t1959` | LP대상종목정보조회 | `market_session` | implemented, not yet recommended |
| `t1960` | ELW등락율상위 | `paginated` | implemented, not yet recommended |
| `t1961` | ELW거래량상위 | `paginated` | implemented, not yet recommended |
| `t1966` | ELW거래대금상위 | `paginated` | implemented, not yet recommended |
| `t1969` | ELW지표검색 | `market_session` | implemented, not yet recommended |
| `t1971` | ELW현재가호가조회 | `market_session` | implemented, not yet recommended |
| `t1972` | ELW현재가(거래원)조회 | `market_session` | implemented, not yet recommended |
| `t1974` | ELW기초자산동일종목 | `market_session` | implemented, not yet recommended |
| `t1988` | 기초자산리스트조회 | `market_session` | implemented, not yet recommended |
| `t2111` | 선물/옵션현재가(시세)조회 | `market_session` | implemented, not yet recommended |
| `t2112` | 선물/옵션현재가호가조회 | `market_session` | implemented, not yet recommended |
| `t2216` | 선물옵션틱분별체결조회차트 | `market_session` | implemented, not yet recommended |
| `t2301` | 옵션전광판 | `market_session` | implemented, not yet recommended |
| `t2522` | 주식선물기초자산조회 | `market_session` | implemented, not yet recommended |
| `t2545` | 상품선물투자자매매동향(챠트용) | `market_session` | implemented, not yet recommended |
| `t3202` | 종목별증시일정 | `market_session` | implemented, not yet recommended |
| `t3320` | FNG_요약 | `market_session` | implemented, not yet recommended |
| `t3341` | 재무순위종합 | `paginated` | implemented, not yet recommended |
| `t3401` | 투자의견 | `paginated` | implemented, not yet recommended |
| `t3518` | 해외실시간지수 | `paginated` | implemented, not yet recommended |
| `t3521` | 해외지수조회(API용) | `market_session` | implemented, not yet recommended |
| `t4203` | 업종차트(종합) | `paginated` | implemented, not yet recommended |
| `t8401` | 주식선물마스터조회(API용) | `market_session` | implemented, not yet recommended |
| `t8402` | 주식선물현재가조회(API용) | `market_session` | implemented, not yet recommended |
| `t8403` | 주식선물호가조회(API용) | `market_session` | implemented, not yet recommended |
| `t8405` | 주식선물기간별주가(API용) | `paginated` | implemented, not yet recommended |
| `t8406` | 주식선물틱분별체결조회(API용) | `market_session` | implemented, not yet recommended |
| `t8407` | API용주식멀티현재가조회 | `market_session` | implemented, not yet recommended |
| `t8410` | API전용주식차트(일주월년) | `paginated` | implemented, not yet recommended |
| `t8411` | 주식차트(틱/n틱) | `paginated` | implemented, not yet recommended |
| `t8412` | 주식 차트(N분봉) 조회 | `paginated` | recommended |
| `t8417` | 업종차트(틱/n틱) | `paginated` | implemented, not yet recommended |
| `t8418` | 업종차트(N분) | `paginated` | implemented, not yet recommended |
| `t8419` | 업종차트(일주월) | `paginated` | implemented, not yet recommended |
| `t8424` | 전체업종 | `market_session` | implemented, not yet recommended |
| `t8425` | 전체테마 | `market_session` | implemented, not yet recommended |
| `t8426` | 상품선물마스터조회(API용) | `market_session` | implemented, not yet recommended |
| `t8430` | 주식종목조회 | `market_session` | implemented, not yet recommended |
| `t8431` | ELW종목조회 | `market_session` | implemented, not yet recommended |
| `t8433` | 지수옵션마스터조회API용 | `market_session` | implemented, not yet recommended |
| `t8434` | 선물/옵션멀티현재가조회 | `market_session` | implemented, not yet recommended |
| `t8435` | 파생종목마스터조회API용 | `market_session` | implemented, not yet recommended |
| `t8436` | 주식종목조회 API용 | `market_session` | implemented, not yet recommended |
| `t8450` | (통합)주식현재가호가조회2 API용 | `market_session` | implemented, not yet recommended |
| `t8451` | (통합)주식챠트(일주월년) API용 | `paginated` | implemented, not yet recommended |
| `t8452` | (통합)주식챠트(N분) API용 | `paginated` | implemented, not yet recommended |
| `t8453` | (통합)주식챠트(틱/N틱) API용 | `paginated` | implemented, not yet recommended |
| `t8464` | 선물옵션차트(틱/n틱) | `paginated` | implemented, not yet recommended |
| `t8465` | 선물/옵션차트(N분) | `paginated` | implemented, not yet recommended |
| `t8466` | 선물/옵션차트(일주월) | `paginated` | implemented, not yet recommended |
| `t8467` | 지수선물마스터조회API용 | `market_session` | implemented, not yet recommended |
| `t9905` | 기초자산리스트조회 | `market_session` | implemented, not yet recommended |
| `t9907` | 만기월조회 | `market_session` | implemented, not yet recommended |
| `t9942` | ELW마스터조회API용 | `market_session` | implemented, not yet recommended |
| `t9943` | 지수선물마스터조회API용 | `market_session` | implemented, not yet recommended |
| `t9944` | 지수옵션마스터조회API용 | `market_session` | implemented, not yet recommended |
| `t9945` | 주식마스터조회API용 | `market_session` | implemented, not yet recommended |
| `token` | 접근토큰 발급 (OAuth2 token issue) | `standalone` | recommended |
