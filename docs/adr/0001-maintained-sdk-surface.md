# Maintained SDK Surface replaces full generated SDK ownership

The old LS SDK architecture treated generated Rust output as the primary SDK surface, but LS API changes are infrequent and the generation pipeline became more expensive to maintain than targeted SDK edits. We will make the Rust SDK code the maintained source of truth, keep API and specification change trackers as advisory inputs, and have agents create reviewed maintenance work items from detected changes instead of routinely regenerating the SDK.
