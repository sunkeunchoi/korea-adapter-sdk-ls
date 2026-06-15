# Focused-Evidence Freshness Rule

This records the freshness policy for **Focused Evidence** on a **Recommended TR**.
It is documentation of intent; only the 90-day backstop is operative in this slice.

## The rule (two controls)

1. **Change-driven invalidation (R8).** A Recommended TR's focused evidence is
   invalidated the moment a tracked spec or documentation change affects that TR.
   This catches the changes a tracker *can* see.

2. **90-day backstop (R9).** Absent any affecting change, focused evidence stays
   valid for **90 days** from `maintenance.last_reviewed`. After that an
   `evidence`-severity finding fires. The backstop catches behavior drift the
   trackers cannot see (session quirks, account-state edge cases). The window is
   a per-class-tightenable default, not a fixed constant — `orders` will want a
   tighter window when its runtime lands.

## What is active in this slice (R10)

Change-driven invalidation (control 1) requires the **Specification Document
Tracker** and its reviewed baselines, which are **not** part of this slice
(`ls-trackers` is deferred). Until that tracker exists, **the 90-day backstop is
the sole operative freshness control**. No change-driven invalidation is wired.

The per-TR `maintenance.last_reviewed` timestamp is the input to the backstop.
