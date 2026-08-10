# GitHub Issues are the Maintenance Work Queue

**Status: superseded.** The work queue is now `queue/items.jsonl`, the single
window-aware staging location described in [`AGENTS.md`](../../AGENTS.md) and
driven by `make next` / `lab-next add|done|supersede`. GitHub Issues remain
useful for discussion and PR linkage, but they are no longer *the* queue: an
agent that stages work there is staging it where nothing will read it. The
decision below is retained for history.

---

Accepted SDK maintenance and expansion work items will live in GitHub Issues, while this repository owns the issue template, label taxonomy, and runbook instructions that make those issues reviewable and repeatable. We chose this over repo-native markdown work-item files because GitHub Issues provide assignment, discussion, filtering, and status handling without building a queue system, while checked-in templates and labels keep the workflow explicit enough for future maintainers to understand and audit.
