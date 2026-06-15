# Staged snapshots for change tracking

The project must keep API and documentation tracking without letting network volatility or temporary upstream changes drive SDK edits directly. Change trackers will fetch upstream LS API and documentation into staged snapshots, normalize and diff those snapshots against reviewed baselines, emit advisory tracker findings, and promote baselines only after review.
