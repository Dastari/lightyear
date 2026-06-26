# Fork extensions — opt-in configurability plan

Status: Planned
Goal: make this fork **upstream-compatible by default** and put every opinionated behavioral
divergence behind an opt-in, with a one-call preset to enable the full set.

## Principle

- **Default build = upstream 0.26.4 behavior + always-on security/correctness hardening.**
- **Opinionated behavior changes are opt-in**, via runtime config on the existing per-subsystem
  config structs, each defaulting to the upstream value.
- A preset (`enable_fork_extensions()` / `ForkExtensions::all()`) flips every opt-in on at once.
- **Acceptance test:** with no opt-ins, the inherited `lightyear_tests` suite's 9 currently-failing
  tests must pass (they fail today only because fork opinions are on). Fork behaviors get their own
  tests run with the opt-ins enabled.

You do **not** make crash-protection or silent bug-fixes opt-in — that would ship known
crashes/bugs by default. "Global opt-in" applies to behavioral opinions only.

## Scope

### Make opt-in (default = upstream)
| Item | Commit | Toggle home | Default |
| --- | --- | --- | --- |
| Input history depth (512) | 792094e0 | `InputConfig.history_depth: u16` | 20 |
| Interpolation clamp / smart-drain / idle-rebase | 6166cd51 | `InterpolationConfig` (`overshoot: Clamp\|Extrapolate`, `idle_rebase: bool`, drain threshold) | Extrapolate, false, upstream |
| Input target authorization (auth half only) | c1d00a90 | opt-in `authorize_controlled_targets` validator on a `ValidateInputs` set | off |
| Seed predicted history on late `Predicted` attach | 3d2d0b71 | `PredictionPlugin`/`RollbackPolicy` flag | off |
| Init confirmed history on late `Interpolated` attach | 0c327008 | `InterpolationConfig` flag | off |
| Bootstrap avian `Transform` on late lane adoption | 87b7dfd9 | `PhysicsTransformConfig` flag | off |

(Decision: late-attach fixes are **opt-in / strict parity**. Input-auth seam is **built fork-local
now**, converging with upstream #1535/#1526 once they merge.)

### Already opt-in — verify defaults + document (no code change)
- Delta keyframes (3f7c3d20) — `keyframe_interval: Option<NonZeroU16>`, default `None`.
- Replication send-metrics observer (c2db90da) — opt-in resource (optionally gate behind `metrics`).
- WebTransport `server_host` (a1eece4c) — `Option<String>`, default `None`.
- f64 avian visual correction (a4981906) — gated by the existing `f64` Cargo feature.
- `ReplicationGroup.send_frequency` — `Option<Timer>`, default `None`.

### Always-on, NOT opt-in (document the rationale)
- **Security:** `split_len` clamp, fragment-metadata validation, Vec/HashMap OOM caps, decode-reject
  metering, and the `end_tick` **lookahead bound** (anti-OOM half of c1d00a90).
- **Non-observable correctness:** delta-ack monotonicity (019048ca), base-diff fallback (0192db9c).
- **Resilience:** UDP backpressure retry (ef11f05e), peer-address eviction on unlink (0ddfd378).
- **Upstream parity (these *are* upstream behavior):** #1473, #1474, #1479, #1471.
- Pure API addition: native input-state sequence visibility (a2fb6731) — keep.

## Mechanism (one idiom)

1. Runtime config fields on the existing structs (`InputConfig` `src/config.rs`, `InterpolationConfig`
   `src/timeline.rs`, `RollbackPolicy` `src/manager.rs`, `PhysicsTransformConfig`), default = upstream.
2. A **fork-local `ValidateInputs` seam** for input auth, shaped like upstream #1535/#1526
   (`authorize_controlled_targets` registered via `add_input_validator`), so it swaps to upstream's
   once merged. Lift c1d00a90's inline `retain` out of `receive_input_message` into that validator;
   keep the lookahead bound in the receive system.
3. A preset `enable_fork_extensions()` (app extension) / `ForkExtensions::all()` that sets every flag
   and registers the validator — the single "global opt-in" switch for consumers.

## Phases

- **P0 — Decisions + scaffolding.** Add the empty `ForkExtensions` preset surface + README/CHANGELOG
  stub. No behavior change.
- **P1 — Tunables → config (low risk).** `HISTORY_DEPTH` → `InputConfig.history_depth` (thread into
  `InputBuffer` sizing). Verify/document the already-opt-in defaults.
- **P2 — Interpolation flags (high impact).** Gate 6166cd51's clamp/drain/idle-rebase and 0c327008's
  observer; default = upstream output.
- **P3 — Input auth via fork-local seam.** `ValidateInputs` set + opt-in `authorize_controlled_targets`;
  lookahead stays always-on. Makes upstream input tests pass by default.
- **P4 — Late-attach prediction/avian flags.** Gate 3d2d0b71 + 87b7dfd9 behind config, default off.
- **P5 — Preset + docs + validation.** Wire `enable_fork_extensions()`; rewrite README "Fork
  additions" → "Fork extensions (opt-in)" (toggles + preset + always-on list).

Each phase: `cargo check --workspace` + relevant tests green. Whole effort validated by
"defaults → the 9 inherited tests pass."

## Follow-ups / converge with upstream
- When #1535 (`ValidateInputs` seam) + #1526 (`authorize_controlled_targets`) merge upstream, replace
  the fork-local seam with the upstream one (and drop c1d00a90's inline path).
- Optionally gate the send-metrics module behind the `metrics` feature for upstream cleanliness.
