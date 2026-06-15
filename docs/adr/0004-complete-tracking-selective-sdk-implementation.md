# Complete tracking with selective SDK implementation

Existing users of the old generated SDK are not a compatibility constraint, so importing all 365 callable TRs would preserve the maintenance burden the new repository is meant to remove. We will track the full LS TR inventory in metadata from the start, but implement Rust SDK behavior selectively by maintainability and usefulness tiers: core foundations first, then standalone/account/market flows, paginated flows, realtime channels, guarded orders, and production-only surfaces only when explicitly needed.
