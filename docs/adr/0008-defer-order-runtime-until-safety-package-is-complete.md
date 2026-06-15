# Defer order runtime until the safety package is complete

Order TRs carry duplicate-submission and ambiguous-outcome risk, so a partial runtime port would be more dangerous than leaving order execution unavailable. The first migration tier will track order TRs in metadata and document order-number coupling and reconciliation, but public order runtime behavior is deferred until no-retry dispatch, deduplication, reconciliation, and guarded focused evidence can ship together.
