# Bevy 0.19 Migration Plan — lightyear-sidereal fork

Status: **DONE** — fork compiles fully on Bevy 0.19 (G1 Sidereal surface + G2 full workspace green);
G3 validated (`lightyear_tests`: 94/104 pass, **zero regressions** vs the 0.18 baseline — the 9 failures
pre-date the migration). Game migration handed off in `GAME_BEVY_0.19_HANDOFF.md`.
Authored: 2026-06-25
Scope of this document: the **fork only** (`/root/lightyear-sidereal`). The companion
handoff for the game (`/root/sidereal`) is produced *from* this migration — see §9.

---

## 1. Strategic context

- The fork **permanently diverges** from upstream Lightyear. Upstream `0.27.0` replaced the
  custom replication backend with `bevy_replicon` (`52e56e61`) and reorganised the workspace
  into `crates/...` (`41b4896e`). **We keep the old custom replication backend and the flat
  workspace layout.** Neither wall is adopted.
- **Upstream is still on Bevy 0.18.** There is *no* upstream reference for a 0.18→0.19
  migration — we do it first. Upside: the 0.19 work is fully independent of the replicon mess.
- This is, in reality, a **two-repo lockstep bump**. The fork cannot ship 0.19 in isolation
  because the game pins both `bevy = "0.18"` *and* this fork by git rev; the instant the fork
  moves to 0.19 the game's dependency tree holds two incompatible Bevy versions. The fork is
  migrated and verified **standalone** first (it is its own Cargo workspace), then handed off.

## 2. Feasibility — green light

| Dependency | Current | Target (Bevy 0.19) | Verified |
| --- | --- | --- | --- |
| `bevy` | 0.18.1 | **0.19** | shipped 2026-06-19 |
| `avian2d` / `avian3d` | 0.6 | **0.7** | supports 0.19 ✅ |
| `aeronet_*` (webtransport/io/ws/steam) | 0.20 | **0.21.0** | supports 0.19, published 2026-06-24 ✅ |
| Rust MSRV | 1.88 | **1.95** | required by aeronet 0.21 |

Deps used only by *non-Sidereal* features (verify availability before enabling those crates):
`bevy_enhanced_input` 0.24 (→ `lightyear_inputs_bei`), `leafwing-input-manager` 0.20
(→ `lightyear_inputs_leafwing`), `bevy-inspector-egui` 0.36 / `bevy_egui` 0.39 (`lightyear_ui` +
examples). **None of these gate the Sidereal feature set** (`udp, raw_connection, input_native,
replication, prediction, interpolation, frame_interpolation, avian2d, webtransport`). Strategy:
update them if 0.19-compatible releases exist; otherwise temporarily exclude those crates/examples
from the workspace build and note it. Sidereal is unaffected either way.

## 3. What is already clean (do NOT spend time here)

Confirmed by repo scan — none of these patterns exist in the fork:
- `Components::resource_id` / `get_valid_resource_id` / `ComponentDescriptor::new_resource`
- `World::remove_resource_by_id` / `IsResource` filters
- `ExecutorKind` / `set_executor_kind` / `get_executor_kind`
- `*_non_send_resource_*` APIs
- `DefaultErrorHandler` / `entities_allocator`
- `bevy_reflect` moved-internals (`DynamicStruct::index_of`, `FieldIter`, `index_of_name`)
- Lifecycle-observer / `HookContext` **exhaustive destructures** — already `On<Add|Insert|Remove|
  Replace>` style (232 `On<…>` sites), accessor-based; the 2 `HookContext` destructures carry `..`.
  The new `old_archetype`/`new_archetype` trigger fields will not break matches.
- Resource-typed `MapEntities` — all `MapEntities` impls are on components/messages.

## 4. The real work — prioritised, file-grounded

### P-1 — `HistoryBuffer<R>` dual `Resource + Component` (HARD BREAK, design decision)
- `lightyear_core/src/history_buffer.rs:44` — `#[derive(Resource, Component, Debug, Reflect)]`
  with `#[reflect(Component, Resource)]` at `:45`. **0.19 forbids deriving both.**
- Used both ways *intentionally*: as a `Component` for component-rollback history, and aliased as
  `ResourceHistory<R> = HistoryBuffer<R>` (`lightyear_prediction/src/resource_history.rs:12`),
  accessed via `ResMut<ResourceHistory<R>>` for resource rollback.
- **Design spike required.** In 0.19 resources *are* components, so options are: (a) keep a single
  `Component` type and express the resource-rollback path over the resource's singleton entity;
  (b) split into two distinct types (`HistoryBuffer` component + `ResourceHistoryBuffer` resource).
  Decide during execution; this unblocks `lightyear_prediction` resource rollback.
- Touch radius: `resource_history.rs:12,21,33,45`, `rollback.rs:689-692`, `prediction/plugin.rs:93`,
  `prediction/registry.rs:457,474`.

### P-2 — `ReflectResource` → `ReflectComponent` (4 reflected resources)
- `lightyear_core/src/history_buffer.rs:45` (`#[reflect(Component, Resource)]`)
- `lightyear_connection/src/client.rs:147` (`PeerMetadata`)
- `lightyear_prediction/src/diagnostics.rs:58` (`PredictionMetrics`)
- `lightyear_ui/src/debug.rs:31` (`MetricsPanelSettings`) — UI crate, non-Sidereal
- `ReflectResource` becomes a ZST in 0.19; route resource reflection through `ReflectComponent`.

### P-3 — `ResMut<R>` / `resource_mut` mutability bound (~28 files)
- 0.19: `ResMut<R>`, `World::resource_mut`, `get_resource_mut` require
  `R: Resource<Mutability = Mutable>`. Concrete code on `#[derive(Resource)]` types is fine; the
  breakage is in **generic** resource code that must add the bound.
- Generic hotspots: `lightyear_prediction/src/{rollback.rs,resource_history.rs,plugin.rs,registry.rs}`.
- 41 `#[derive(Resource)]` types total; sweep is compiler-driven.

### P-4 — Raw erased access / `UnsafeWorldCell` / `StorageType` API drift (compile-driven)
All key off `ComponentId` and filter to replication-marked entities, so **semantically safe**
(resource singleton entities won't be matched). Risk is pure signature drift in 0.19:
- `lightyear_utils/src/ecs.rs:14-67` — core erased fetch (`StorageType::{Table,SparseSet}`,
  `Table::get_component`, sparse-set `get`, `assert_unique`). **Highest-churn single file.**
- `lightyear_deterministic_replication/src/archetypes.rs` — `UnsafeWorldCell`,
  `archetype.get_storage_type`, `ArchetypeGeneration` walking, manual `FilteredAccess`.
- `lightyear_replication/src/registry/{buffered.rs,delta.rs,registry.rs,deterministic.rs}` —
  `OwningPtr`/`Ptr`/`PtrMut`, `insert_by_ids`/`remove_by_ids`, erased fn-pointers.
- `lightyear_replication/src/send/{buffer.rs,components.rs,sender.rs}` — `EntityRef::get_by_id`,
  `get_change_ticks_by_id`, `as_unsafe_world_cell` + `get_components_mut_unchecked`.
- `lightyear_replication/src/receive.rs` — `as_unsafe_world_cell()`/`world_mut()` aliasing.
- `lightyear_messages/src/{receive.rs,send.rs}` — `get_mut_by_id`/`get_by_id`.
- `lightyear_serde/src/registry.rs:298`, `lightyear_prediction/src/registry.rs` — `PtrMut`,
  `FilteredEntityMut`.

### P-5 — `Access` API renames (3 sites)
- `lightyear_deterministic_replication/src/archetypes.rs:128,139,147` —
  `add_component_read`/`add_component_write` → `add_read`/`add_write`.

### P-6 — Misc renames + general compile sweep (compiler-driven, all 30 crates)
- `System::type_id()` → `system_type()`, `Ref` now `Clone`/`Copy` (inner clone needs
  `.as_ref().clone()`), and any other 0.19 renames the compiler flags. Drive from `cargo check`.

## 5. Scale reference (core crate LOC)

`lightyear_replication` 12.0k · `lightyear_transport` 5.7k · `lightyear_netcode` 4.8k ·
`lightyear_prediction` 2.8k · `lightyear_sync` 2.4k · `lightyear_serde` 2.1k ·
`lightyear_inputs` 2.6k · `lightyear_core` 1.9k · `lightyear_interpolation` 1.5k ·
`lightyear_connection` 1.3k. The migration concentrates in `replication`, `core`, `prediction`,
and the shared `lightyear_utils/ecs.rs` helper.

## 6. Upstream cherry-picks (clean only)

Do these **before** the dep bump, on the Bevy 0.18 base, so they apply cleanly; then migrate.
- `ce5afba2` — interpolation "use try commands" (despawn-race panic fix). Clean, 3 files.
- `9f53387a` — packet-size overflow guard. **Transport hunks only**; drop its `lightyear_replication`
  hunks (replicon-coupled).

Parked as a separate, post-migration backlog item (NOT part of this campaign): upstream prediction
rollback fixes `#1512` and `#1492`. They cannot be `git cherry-pick`-ed because upstream built them
on a prediction-history refactor (`ConfirmedHistory`/`checkpoint_ticks`) this fork never adopted;
adopting them means a manual logic port. Orthogonal to Bevy 0.19.

## 7. Divergence formalisation (one-time)

- Create the permanent mainline branch `sidereal/main` from current HEAD (`ca6ef037`).
- Keep `upstream` as fetch-only (already `push = DISABLED`); cherry-pick deliberately, never merge.
- Version stamp **deferred**: bumping `workspace.package.version` to a pre-release (e.g.
  `0.26.4-sidereal.1`) forces updating every internal path-dep `version = "0.26.4"` requirement
  (semver pre-release matching), which is pure churn since the game resolves by git rev. The branch
  name `sidereal/main` + this plan are the divergence markers. Revisit only if a human-facing
  version identity is wanted later.
- Keep the **flat** workspace layout. Do not adopt upstream's `crates/` reorg.

## 8. Phases & verification gates

- **Phase 0 — Formalise + cherry-pick (Bevy 0.18).** §7 + §6. Gate G0: `cargo check` green on 0.18.
  (A clean checkpoint; optional given "no rollback needed", but cheap and de-risks the cherry-picks.)
- **Phase 1 — Dependency bump.** Ensure Rust 1.95 toolchain. Bump `Cargo.toml`: bevy 0.19,
  avian 0.7, aeronet 0.21, MSRV 1.95; resolve non-Sidereal ecosystem deps or exclude their crates.
  `cargo check` to surface the full breakage wall.
- **Phase 2 — Fix breakage** in priority order P-1 → P-6, crate by crate.
  - Gate G1: `cargo check -p lightyear --no-default-features --features "udp raw_connection
    input_native replication prediction interpolation frame_interpolation avian2d webtransport
    webtransport_self_signed"` green (the exact Sidereal surface).
  - Gate G2: `cargo check --workspace` green (or documented list of excluded non-Sidereal crates).
  - Gate G3: `cargo test -p lightyear_tests` green.
- **Phase 3 — Handoff.** Produce the game handoff doc (§9), capturing the *actual* public-API delta
  observed during Phase 2. The game re-pin + migration is done later by a fresh agent.

## 9. Game handoff doc (§Doc B — produced during Phase 2/3, lives in the fork: `GAME_BEVY_0.19_HANDOFF.md`)

The fresh agent migrating `/root/sidereal` needs more than version numbers. Outline:
1. **Version bumps**: bevy 0.18→0.19, avian2d 0.6→0.7, `bevy_remote` 0.18→0.19, MSRV 1.95, re-pin
   `lightyear` rev to the new `sidereal/main` HEAD.
2. **Lightyear public-API delta** (filled live during Phase 2): every changed public symbol /
   signature / trait bound the game's code touches. This is the high-value 90%.
3. **General game-side 0.18→0.19 hazards**: resources-as-components hits the game's own 25 crates
   directly (ReflectResource, ResMut mutability bound, broad `Query<Entity>` vs resource entities,
   `#[derive(Resource)]` that also wants Component). Pointer to the official migration guide plus the
   specific items the fork migration proved relevant.
4. **Framing note**: this handoff is *Lightyear-specific notes layered on top of* a normal Bevy
   0.18→0.19 game migration — not the whole job.

## 10. Risks & open questions

- **HistoryBuffer split (P-1)** is a genuine design decision, not a mechanical edit. Spike first.
- **Raw-pointer API drift (P-4)** is only fully knowable by compiling against 0.19; estimate is a
  band, not a point.
- **Non-Sidereal ecosystem deps** (bei/leafwing/egui) may lack 0.19 releases this early; have an
  exclude-and-note fallback so they never block the Sidereal-critical path.
- **Rust 1.95 toolchain** must be installed in the environment before Phase 1.
- The fork's tests cover most of its API but not the game's exact usage; residual game-specific
  friction is expected and surfaces via the game's compiler in the later migration.
