//! Drift gate (U5 ⇄ U8): the hand-authored `{TR}_POLICY` runtime consts in
//! `endpoint_policy.rs` are the runtime mirror of the maintained metadata index.
//! This test cross-checks each slice TR's const against the authored
//! `metadata/` set so the two cannot silently diverge — the const's `tr_code`
//! must be indexed, its `protocol` must match the index, and its rate-limit
//! `category` must match the per-TR `facets.rate_bucket`.

use std::path::PathBuf;

use ls_core::endpoint_policy::{
    CCENQ10100_POLICY, CCENQ90200_POLICY, CFOAQ10100_POLICY, CFOBQ10500_POLICY, CSPAQ12200_POLICY,
    CSPAQ12300_POLICY, CSPAQ22200_POLICY, CSPAT00601_POLICY, CSPAT00701_POLICY, CSPAT00801_POLICY,
    T0424_POLICY, T0167_POLICY, CSPBQ00200_POLICY, CLNAQ00100_POLICY, CFOEQ11100_POLICY, T0441_POLICY, CIDBQ01400_POLICY,
    CIDBQ03000_POLICY, CIDBQ05300_POLICY,
    T0425_POLICY, K3_POLICY, REVOKE_POLICY,
    FC9_POLICY, FH9_POLICY, GSC_POLICY, GSH_POLICY, H1_POLICY, HA_POLICY, OC0_POLICY, OH0_POLICY,
    OVC_POLICY, OVH_POLICY, S2_POLICY, UH1_POLICY, US2_POLICY, US3_POLICY,
    AS0_POLICY, AS1_POLICY, AS2_POLICY, AS3_POLICY, AS4_POLICY, C01_POLICY, H01_POLICY, O01_POLICY,
    SC0_POLICY, SC1_POLICY, SC2_POLICY, SC3_POLICY, SC4_POLICY, TC1_POLICY, TC2_POLICY, TC3_POLICY,
    S3_POLICY,
    T1101_POLICY, T1102_POLICY,
    T1403_POLICY, T1441_POLICY, T1452_POLICY, T1463_POLICY, T1466_POLICY, T1481_POLICY,
    T1482_POLICY, T1489_POLICY, T1492_POLICY,
    T1601_POLICY, T1615_POLICY, T1640_POLICY, T1662_POLICY, T1664_POLICY, T1531_POLICY,
    T1537_POLICY, T1825_POLICY, T1826_POLICY, T1859_POLICY, T1866_POLICY, T1958_POLICY,
    T1310_POLICY, T1404_POLICY, T1410_POLICY, T1411_POLICY, T1488_POLICY, T1636_POLICY, T1809_POLICY,
    T1109_POLICY, T1301_POLICY, T1486_POLICY, T8454_POLICY, T1637_POLICY,
    T1602_POLICY, T1603_POLICY, T1617_POLICY, T1752_POLICY, T1771_POLICY,
    T1485_POLICY, T1511_POLICY, T1514_POLICY, T1516_POLICY, T1901_POLICY, T1906_POLICY, T8450_POLICY, T1638_POLICY, T1308_POLICY, T1449_POLICY, T1621_POLICY, T2545_POLICY, T8406_POLICY, T8407_POLICY, T1631_POLICY, T1632_POLICY, T1633_POLICY, T1716_POLICY, T1902_POLICY, T1904_POLICY, T1927_POLICY, T1941_POLICY, T1702_POLICY, T1717_POLICY, T1665_POLICY, T1471_POLICY, T1475_POLICY, T1959_POLICY, T1950_POLICY, T1954_POLICY, T1971_POLICY, T1972_POLICY, T1974_POLICY, T1956_POLICY, T1969_POLICY, T1105_POLICY, T1104_POLICY,
    T1305_POLICY, T1964_POLICY, T2301_POLICY,
    T2522_POLICY, T3341_POLICY,
    T8401_POLICY, T8426_POLICY, T8433_POLICY, T8435_POLICY, T8467_POLICY, T9943_POLICY, T9944_POLICY,
    T2106_POLICY, T2111_POLICY, T2112_POLICY, T8402_POLICY, T8403_POLICY, T8434_POLICY,
    T1988_POLICY, T3102_POLICY, T3320_POLICY, T8455_POLICY, T8460_POLICY, T8463_POLICY,
    T9945_POLICY, T3202_POLICY, T3401_POLICY, T8410_POLICY, T8451_POLICY, T8419_POLICY,
    T4203_POLICY, T3518_POLICY, T3521_POLICY,
    O3103_POLICY, O3104_POLICY, O3108_POLICY, O3116_POLICY, O3117_POLICY, O3123_POLICY,
    O3127_POLICY, O3128_POLICY, O3136_POLICY, O3137_POLICY, O3139_POLICY, T8462_POLICY,
    T8427_POLICY, T2210_POLICY, T2424_POLICY, T2541_POLICY, T2214_POLICY, T8428_POLICY,
    T8417_POLICY, T8418_POLICY, T8411_POLICY, T8452_POLICY, T8453_POLICY, T1302_POLICY,
    T8464_POLICY, T8465_POLICY, T8466_POLICY, T8405_POLICY, T2216_POLICY,
    T1444_POLICY, T1422_POLICY, T1427_POLICY, T1442_POLICY, T1405_POLICY, T1960_POLICY, T1961_POLICY, T1966_POLICY, T1921_POLICY, T1532_POLICY, T1533_POLICY, T1926_POLICY, T1764_POLICY, T1903_POLICY,
    G3101_POLICY, G3102_POLICY, G3103_POLICY, G3104_POLICY, G3106_POLICY, G3190_POLICY,
    O3101_POLICY, O3105_POLICY, O3106_POLICY, O3121_POLICY, O3125_POLICY, O3126_POLICY,
    T8412_POLICY, T8424_POLICY, T8425_POLICY, T8430_POLICY, T8431_POLICY, T8436_POLICY,
    T9905_POLICY, T9907_POLICY, T9942_POLICY, TOKEN_POLICY,
    // Closure-flip WS batch (plan -004): 31 connection-reachable-only WS channels.
    NS3_POLICY, NH1_POLICY, NS2_POLICY, NK1_POLICY, NBT_POLICY, KS_POLICY, OK_POLICY, KH_POLICY,
    KM_POLICY, PH_POLICY, K1_POLICY, IJ_POLICY, YS3_POLICY, YK3_POLICY, VI_POLICY, JC0_POLICY,
    JH0_POLICY, JD0_POLICY, FD0_POLICY, OD0_POLICY, OMG_POLICY, YF9_POLICY, YOC_POLICY, BM_POLICY,
    WOC_POLICY, WOH_POLICY, JIF_POLICY, NWS_POLICY, BMT_POLICY, CUR_POLICY, MK2_POLICY,
    // Open-window WS track-flip wave (plan 2026-06-29-001): 39 connection-reachable-only WS channels.
    AFR_POLICY, B7_POLICY, C02_POLICY, CD0_POLICY, DBM_POLICY, DBT_POLICY, DC0_POLICY, DD0_POLICY,
    DH0_POLICY, DH1_POLICY, DHA_POLICY, DK3_POLICY, DS3_POLICY, DVI_POLICY, ESN_POLICY, FX9_POLICY,
    H02_POLICY, H2_POLICY, HB_POLICY, I5_POLICY, JX0_POLICY, NBM_POLICY, NPM_POLICY, NVI_POLICY,
    O02_POLICY, OX0_POLICY, SHC_POLICY, SHD_POLICY, SHI_POLICY, SHO_POLICY, UBM_POLICY, UBT_POLICY,
    UK1_POLICY, UVI_POLICY, UYS_POLICY, YC3_POLICY, YJC_POLICY, YJ_POLICY, H3_POLICY,
};
use ls_core::{EndpointPolicy, Protocol, RateLimitCategory};
use ls_metadata::{validate_dir, Protocol as MetaProtocol, RateBucket};

fn metadata_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("metadata")
}

fn protocol_matches(runtime: Protocol, meta: MetaProtocol) -> bool {
    matches!(
        (runtime, meta),
        (Protocol::Rest, MetaProtocol::Rest) | (Protocol::WebSocket, MetaProtocol::Websocket)
    )
}

fn category_matches(runtime: RateLimitCategory, meta: RateBucket) -> bool {
    matches!(
        (runtime, meta),
        (RateLimitCategory::MarketData, RateBucket::MarketData)
            | (RateLimitCategory::Orders, RateBucket::Orders)
            | (RateLimitCategory::Account, RateBucket::Account)
            | (RateLimitCategory::Auth, RateBucket::Auth)
    )
}

#[test]
fn slice_policies_mirror_metadata_index() {
    let report = validate_dir(&metadata_root())
        .unwrap_or_else(|e| panic!("metadata failed to validate: {e:?}"));

    let policies: &[EndpointPolicy] = &[
        TOKEN_POLICY,
        REVOKE_POLICY,
        T1101_POLICY,
        T1102_POLICY,
        T8412_POLICY,
        T8425_POLICY,
        T8436_POLICY,
        T1531_POLICY,
        T1537_POLICY,
        T1452_POLICY,
        T1481_POLICY,
        T1482_POLICY,
        T1403_POLICY,
        T1441_POLICY,
        T1463_POLICY,
        T1466_POLICY,
        T1489_POLICY,
        T1492_POLICY,
        T1859_POLICY,
        T1866_POLICY,
        T1825_POLICY,
        T1826_POLICY,
        T9905_POLICY,
        T9907_POLICY,
        T8431_POLICY,
        T8430_POLICY,
        T9942_POLICY,
        T1958_POLICY,
        T1964_POLICY,
        T1601_POLICY,
        T1615_POLICY,
        T1640_POLICY,
        T1662_POLICY,
        T1664_POLICY,
        T3341_POLICY,
        T8424_POLICY,
        T1511_POLICY,
        T1485_POLICY,
        T1516_POLICY,
        T1514_POLICY,
        T1901_POLICY,
        T1906_POLICY,
        T8450_POLICY,
        T1638_POLICY,
        T1308_POLICY,
        T1449_POLICY,
        T1621_POLICY,
        T2545_POLICY,
        T8406_POLICY,
        T8407_POLICY,
        T1631_POLICY,
        T1632_POLICY,
        T1633_POLICY,
        T1716_POLICY,
        T1902_POLICY,
        T1904_POLICY,
        T1927_POLICY,
        T1941_POLICY,
        T1702_POLICY,
        T1717_POLICY,
        T1665_POLICY,
        T1471_POLICY,
        T1475_POLICY,
        T1959_POLICY,
        T1950_POLICY,
        T1954_POLICY,
        T1971_POLICY,
        T1972_POLICY,
        T1974_POLICY,
        T1956_POLICY,
        T1969_POLICY,
        T1105_POLICY,
        T1104_POLICY,
        T1305_POLICY,
        // Closed-window flip wave (plan -003): self-paginated stock reads.
        T1310_POLICY,
        T1404_POLICY,
        // Closed-window more-flips wave (plan -001): self-paginated stock read.
        T1410_POLICY,
        // Closed-window more-flips wave (plan -001): self-paginated stock read (margin rates).
        T1411_POLICY,
        // Closed-window more-flips wave (plan -001): self-paginated stock read (expected-exec).
        T1488_POLICY,
        // Closed-window more-flips wave (plan -001): self-paginated stock read (program-trading).
        T1636_POLICY,
        // Closed-window more-flips wave (plan -001): self-paginated stock read (signal search).
        T1809_POLICY,
        // Open-window domestic reads (plan -001): self-paginated stock tick/conclusion
        // + program-flow reads.
        T1109_POLICY,
        T1301_POLICY,
        T1486_POLICY,
        T8454_POLICY,
        T1637_POLICY,
        // Open-window domestic reads (plan -001): self-paginated investor-flow
        // + exchange-broker reads.
        T1602_POLICY,
        T1603_POLICY,
        T1617_POLICY,
        T1752_POLICY,
        T1771_POLICY,
        CSPAQ12200_POLICY,
        CSPAQ12300_POLICY,
        CSPAQ22200_POLICY,
        // Closed-window account-lane flip wave (plan -001): non-order account reads.
        T0424_POLICY,
        CSPBQ00200_POLICY,
        CLNAQ00100_POLICY,
        CFOEQ11100_POLICY,
        T0441_POLICY,
        CIDBQ01400_POLICY,
        // Paper account credential lanes (plan -002): overseas-F/O account reads.
        CIDBQ03000_POLICY,
        CIDBQ05300_POLICY,
        // Closed-window account-lane flip wave (plan -001): server-time utility.
        T0167_POLICY,
        // Order policies (is_order: true). Registered HERE only — they must NOT
        // appear in `slice_rest_policies_are_non_order_rest` (R12).
        CSPAT00601_POLICY,
        CSPAT00701_POLICY, // modify
        CSPAT00801_POLICY, // cancel
        // t0425 reconciliation read (is_order: false) — registered in BOTH lists.
        T0425_POLICY,
        CFOBQ10500_POLICY,
        CCENQ90200_POLICY,
        CFOAQ10100_POLICY,
        CCENQ10100_POLICY,
        T2301_POLICY,
        T2522_POLICY,
        T8401_POLICY,
        T8426_POLICY,
        T8433_POLICY,
        T8435_POLICY,
        T8467_POLICY,
        T9943_POLICY,
        T9944_POLICY,
        T2111_POLICY,
        T2112_POLICY,
        T2106_POLICY,
        T8402_POLICY,
        T8403_POLICY,
        T8434_POLICY,
        T1988_POLICY,
        T3102_POLICY,
        T3320_POLICY,
        // Domestic stock master/reference breadth wave (plan -004).
        T9945_POLICY,
        T3202_POLICY,
        T3401_POLICY,
        T3518_POLICY,
        T3521_POLICY,
        O3103_POLICY,
        O3104_POLICY,
        O3108_POLICY,
        O3116_POLICY,
        O3117_POLICY,
        O3123_POLICY,
        O3127_POLICY,
        O3128_POLICY,
        O3136_POLICY,
        O3137_POLICY,
        O3139_POLICY,
        T8427_POLICY,
        T2210_POLICY,
        T2424_POLICY,
        T2541_POLICY,
        T2214_POLICY,
        T8428_POLICY,
        T8462_POLICY,
        T8410_POLICY,
        T8451_POLICY,
        T8419_POLICY,
        T4203_POLICY,
        T8417_POLICY,
        T8418_POLICY,
        T8411_POLICY,
        T8452_POLICY,
        T8453_POLICY,
        T1302_POLICY,
        T8464_POLICY,
        T8465_POLICY,
        T8466_POLICY,
        T8405_POLICY,
        T2216_POLICY,
        T1444_POLICY,
        T1422_POLICY,
        T1427_POLICY,
        T1442_POLICY,
        T1405_POLICY,
        T1960_POLICY,
        T1961_POLICY,
        T1966_POLICY,
        T1921_POLICY,
        T1532_POLICY,
        T1533_POLICY,
        T1926_POLICY,
        T1764_POLICY,
        T1903_POLICY,
        T8455_POLICY,
        T8460_POLICY,
        T8463_POLICY,
        G3101_POLICY,
        G3104_POLICY,
        G3106_POLICY,
        G3102_POLICY,
        G3103_POLICY,
        G3190_POLICY,
        O3101_POLICY,
        O3121_POLICY,
        O3105_POLICY,
        O3106_POLICY,
        O3125_POLICY,
        O3126_POLICY,
        K3_POLICY,
        H1_POLICY,
        HA_POLICY,
        S2_POLICY,
        US3_POLICY,
        UH1_POLICY,
        US2_POLICY,
        GSC_POLICY,
        GSH_POLICY,
        OVC_POLICY,
        OVH_POLICY,
        OC0_POLICY,
        OH0_POLICY,
        FC9_POLICY,
        FH9_POLICY,
        // P2 order-event lane (observation-only 주문/체결 feeds).
        SC0_POLICY,
        SC1_POLICY,
        SC2_POLICY,
        SC3_POLICY,
        SC4_POLICY,
        C01_POLICY,
        O01_POLICY,
        H01_POLICY,
        AS0_POLICY,
        AS1_POLICY,
        AS2_POLICY,
        AS3_POLICY,
        AS4_POLICY,
        TC1_POLICY,
        TC2_POLICY,
        TC3_POLICY,
        S3_POLICY,
        // Closure-flip WS batch (plan -004): 31 connection-reachable-only WS channels.
        NS3_POLICY,
        NH1_POLICY,
        NS2_POLICY,
        NK1_POLICY,
        NBT_POLICY,
        KS_POLICY,
        OK_POLICY,
        KH_POLICY,
        KM_POLICY,
        PH_POLICY,
        K1_POLICY,
        IJ_POLICY,
        YS3_POLICY,
        YK3_POLICY,
        VI_POLICY,
        JC0_POLICY,
        JH0_POLICY,
        JD0_POLICY,
        FD0_POLICY,
        OD0_POLICY,
        OMG_POLICY,
        YF9_POLICY,
        YOC_POLICY,
        BM_POLICY,
        WOC_POLICY,
        WOH_POLICY,
        JIF_POLICY,
        NWS_POLICY,
        BMT_POLICY,
        CUR_POLICY,
        MK2_POLICY,
        // Open-window WS track-flip wave (plan 2026-06-29-001): 39 connection-reachable-only.
        AFR_POLICY,
        B7_POLICY,
        C02_POLICY,
        CD0_POLICY,
        DBM_POLICY,
        DBT_POLICY,
        DC0_POLICY,
        DD0_POLICY,
        DH0_POLICY,
        DH1_POLICY,
        DHA_POLICY,
        DK3_POLICY,
        DS3_POLICY,
        DVI_POLICY,
        ESN_POLICY,
        FX9_POLICY,
        H02_POLICY,
        H2_POLICY,
        HB_POLICY,
        I5_POLICY,
        JX0_POLICY,
        NBM_POLICY,
        NPM_POLICY,
        NVI_POLICY,
        O02_POLICY,
        OX0_POLICY,
        SHC_POLICY,
        SHD_POLICY,
        SHI_POLICY,
        SHO_POLICY,
        UBM_POLICY,
        UBT_POLICY,
        UK1_POLICY,
        UVI_POLICY,
        UYS_POLICY,
        YC3_POLICY,
        YJC_POLICY,
        YJ_POLICY,
        H3_POLICY,
    ];

    for policy in policies {
        let entry = report.index.trs.get(policy.tr_code).unwrap_or_else(|| {
            panic!(
                "runtime const {}_POLICY has tr_code `{}` that is not in tr-index.yaml",
                policy.tr_code.to_uppercase(),
                policy.tr_code
            )
        });
        assert!(
            protocol_matches(policy.protocol, entry.protocol),
            "TR `{}`: runtime protocol {:?} disagrees with index protocol {:?}",
            policy.tr_code,
            policy.protocol,
            entry.protocol
        );

        let meta = report
            .trs
            .get(policy.tr_code)
            .expect("validated TR must have a per-TR record");
        assert!(
            category_matches(policy.category, meta.facets.rate_bucket),
            "TR `{}`: runtime rate category {:?} disagrees with facets.rate_bucket {:?}",
            policy.tr_code,
            policy.category,
            meta.facets.rate_bucket
        );

        // self_paginated ⟹ has_pagination: a TR whose result self-paginates MUST
        // have the policy thread the `tr_cont`/`tr_cont_key` continuation, else a
        // paginated TR ships with single-page dispatch silently. This is a one-way
        // implication, NOT equality: `CSPAQ12200` threads the header cursor
        // defensively (has_pagination=true) while its balance result is single-page
        // (self_paginated=false), and both are intentional.
        if meta.facets.self_paginated {
            assert!(
                policy.has_pagination,
                "TR `{}`: facets.self_paginated is true but runtime has_pagination is false \
                 — a self-paginating TR must thread continuation",
                policy.tr_code
            );
        }
    }
}
