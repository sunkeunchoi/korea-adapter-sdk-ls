//! `lab-next` queue edit-surface tests (U1, R6/R8/R9; KTD2/KTD3/KTD6; AE3)
//! plus the default window-aware report (U5, R1/R2/R4/R5/R12/R13; KTD1).
//! Every run is the compiled bin as a subprocess so env is isolated; the queue
//! file is a tempdir path via `LS_QUEUE_PATH` (KTD2's test-time override).


// A test target's crate root resolves `mod` against `tests/`, so each child needs
// its path spelled out. The children stay in `tests/next_cli/`, which cargo does
// not treat as a test target, so this suite remains ONE test binary.
#[path = "next_cli/edit_surface.rs"]
mod edit_surface;
#[path = "next_cli/fixture.rs"]
mod fixture;
#[path = "next_cli/priority_and_blocked.rs"]
mod priority_and_blocked;
#[path = "next_cli/report.rs"]
mod report;
#[path = "next_cli/standing.rs"]
mod standing;
