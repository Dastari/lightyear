# Bevy 0.19 migration — handoff for the Sidereal game (`/root/sidereal`)

Audience: the agent migrating the **game** workspace (`/root/sidereal`, ~25 crates + 4 bins) to
Bevy 0.19. The Lightyear fork (`/root/lightyear-sidereal`, branch `sidereal/main`) is **already on
Bevy 0.19** and fully compiles + is regression-free; this doc tells you what the game must change.

**Framing:** this is Lightyear-specific notes layered on top of a *normal* Bevy 0.18→0.19 game
migration. The bulk of your work is the general Bevy churn in the game's own 25 crates (§3), not the
Lightyear API (§2, which is tiny). Read the official guide first:
https://bevy.org/learn/migration-guides/0-18-to-0-19/

---

## 1. Dependency + toolchain bumps (game `Cargo.toml`)

| Dep | From | To | Notes |
| --- | --- | --- | --- |
| `bevy` | 0.18.0 | **0.19** | |
| `bevy_remote` | 0.18.0 | **0.19** | |
| `avian2d` | 0.6 | **0.7** | keep your `2d, f64, parry-f64, parallel, serialize, xpbd_joints` features |
| `lightyear` (git rev) | `ca6ef03…` | **`sidereal/main` HEAD** (currently `4b2e1937…`) | re-pin to the migrated fork; keep the same feature list |
| Rust toolchain (MSRV) | 1.88 | **1.95** | required by aeronet 0.21 (pulled via the fork). Ensure 1.95 is installed. |

Also bump any *other* Bevy-coupled deps the game has (egui / bevy_egui / inspector-egui / leafwing /
bevy_enhanced_input, if present) to their 0.19-compatible releases. For reference, the fork resolved:
`bevy_egui 0.40`, `egui 0.34`, `egui_extras 0.34`, `bevy-inspector-egui 0.37`, `leafwing 0.21`,
`bevy_enhanced_input 0.26`. The whole graph resolved cleanly — no version pinning gymnastics needed.

Note: the fork keeps its **own custom replication backend** (NOT `bevy_replicon`). Do not pull
`bevy_replicon`; nothing in the game should reference it.

---

## 2. Lightyear public-API delta (what the game's calls into lightyear will hit)

The migration was almost entirely internal. The only public surface change the game can hit:

- **`add_resource_rollback::<R>()` now requires `R: Resource<Mutability = Mutable>`.**
  If the game registers resource rollback for any resource, that resource must be mutable (normal
  resources are; this only bites an `#[component(immutable)]` resource, which would be unusual for
  rollback). No code change needed unless you rollback an immutable resource.

- `WebTransportClientIo` has a `server_host: Option<String>` field (fork patch, predates 0.19).
  The rev you currently pin already has it, so this is not new — but if you construct
  `WebTransportClientIo` directly anywhere, include `server_host` (use `None` to connect by `PeerAddr`).

Everything else the game uses (replication, prediction, `input_native`, interpolation, the avian2d
binding, webtransport) kept its public signatures.

---

## 3. General Bevy 0.18→0.19 hazards in the game's own code

These are the changes the fork actually hit; expect the same classes across the game's 25 crates.
Ordered roughly by how often they bite.

### ECS (engine-ecs, engine-core, sidereal-game, gameplay, net, persistence)
- **Resources-as-components (the big one).** `#[derive(Resource)]` now also implements `Component`
  (with **SparseSet storage** + an auto-required **`IsResource`** marker). Consequences:
  - A type cannot derive **both** `Resource` and `Component` — pick one. If a type is genuinely used
    as both (rare), split it into two types (the fork did this for its history buffer: a `Component`
    type for per-entity data + a separate `Resource` newtype that `Deref`s to it).
  - `ReflectResource` is now a ZST. The `#[reflect(Resource)]` *attribute still compiles fine*; only
    code that *uses* `ReflectResource` to reflect-access a resource needs to move to `ReflectComponent`.
  - Broad queries (`Query<Entity>`, `Query<()>`) can now match resource singleton entities — add
    `Without<IsResource>` if you iterate "all entities" and don't want resources.
- **`ResMut<R>` / `World::resource_mut` require `R: Resource<Mutability = Mutable>`.** Concrete code is
  fine; **generic** functions over `<R: Resource>` that take `ResMut<R>` must add the bound:
  `<R: Resource<Mutability = bevy_ecs::component::Mutable>>`.
- **Lifecycle `Replace` → `Discard`.** `#[component(on_replace = …)]` → `on_discard`; the trigger type
  `Replace` → `Discard` (so `On<Replace, C>` → `On<Discard, C>`). `#[doc(alias="Replace")]` is kept.
- **`Ref` now implements `Clone`.** `some_ref.clone()` now clones the `Ref` wrapper, not the inner
  value. Use `(*some_ref).clone()` (or `.into_inner().clone()`) to clone `T`.
- **`Command` gained `type Out`.** Manual `impl Command for X` needs `type Out = ();` (apply still
  returns it).
- **`QueryData` split out `IterQueryData`.** Generic code that calls `.iter()/.iter_mut()` over a
  query whose data is an associated/generic type now needs that type bounded `: IterQueryData`.
- **`SystemParam::get_param` returns `Result<Self::Item, SystemParamValidationError>`** (validation
  moved to fetch time). Manual `SystemParam` impls must return `Ok(..)`.
- **`Access`/`FilteredAccess` renames:** `add_component_read/write` → `add_read/write`,
  `has_component_read/write` → `has_read/write`.
- **`Archetype` is no longer `Sync`** (`RequiredComponentConstructor` holds a `!Sync Arc<dyn Fn>`).
  Closures captured by `par_iter().for_each(..)` that hold an `EntityRef`/`FilteredEntityRef` (or
  anything reaching archetype data) no longer satisfy `Sync` — iterate sequentially there, or restructure
  so the parallel closure doesn't capture the entity-ref.

### Rendering / text / lights (engine-render, engine-ui, shader-preview, genesis/thumbnails)
- **`TextFont::font_size` is now the `FontSize` enum**, not `f32`. Use `FontSize::Px(10.0)`
  (in the bevy prelude). `TextFont::from_font_size(f32)` still takes an `f32`.
- **`PointLight::shadows_enabled` → `shadow_maps_enabled`** (and check `DirectionalLight`/`SpotLight`).
- **bevy_render reorg** — expect moved items: `bevy_material` crate extracted from `bevy_pbr`/
  `bevy_render`; `Hdr` moved `bevy_render`→`bevy_camera`; light APIs moved to `bevy_light`; `HalfSpace`
  `bevy_camera`→`bevy_math`. The game's custom rendering/material/shader code (Lighting V2, post-fx,
  screen effects) is the most likely place to hit these — budget time and use the migration guide's
  render section as a checklist.

### Physics (engine-physics, engine-spatial, anything using avian directly)
- **avian 0.7 dropped `PhysicsStepSystems::SpatialQuery`.** Spatial-query updates moved to the dedicated
  `SpatialQuerySystems` set, which now runs in **`FixedPostUpdate`** (not inside `PhysicsSchedule`). If
  the game orders systems relative to spatial queries, retarget to `SpatialQuerySystems` and account for
  the schedule move. (The fork's lag-compensation has a TODO here too.)
- Other avian 0.6→0.7 API changes may surface in physics glue; consult avian's changelog.

### Schedules / executor / misc
- `System::type_id()` → `system_type()`; `DefaultErrorHandler` → `FallbackErrorHandler`;
  `World::entities_allocator()` → `entity_allocator()`. (The fork didn't use these, but the game might.)

---

## 4. Validation expectations

- The fork's `cargo check --workspace` is green on 0.19; its Sidereal feature surface compiles cleanly.
- **Pre-existing fork test failures (NOT caused by 0.19):** `lightyear_tests` has **9 failing tests**
  on this fork (prediction/input/delta/rollback) that fail identically on Bevy 0.18 and 0.19 — they are
  fork-divergence vs. inherited upstream tests, not migration regressions. If the game's CI runs the
  fork's test suite, expect these 9; don't chase them as part of the 0.19 work. (94/104 pass; the
  migration actually fixed one previously-failing test.)
- After the game compiles on 0.19, do a live-play smoke test (prediction/rollback/interpolation are the
  fork's customized hot paths and warrant runtime eyes, per the project's physics-desync history).

---

## 5. Fork migration commit trail (for reference)

On `sidereal/main`, on top of the pre-migration HEAD `ca6ef037`:
```
b5e539fd  migration plan doc
d83362b1  Phase 0: interpolation cherry-pick (ce5afba2)
d0e0c1a5  Phase 0: transport cherry-pick (#1490)
d89e053e  Bevy 0.19 — Sidereal feature surface (Gate G1)
5ea38d89  lightyear_ui font_size
4b2e1937  full workspace green (Gate G2)  <-- re-pin the game to this (or later) rev
```
The fork is NOT pushed yet — push `sidereal/main` to the `Dastari/lightyear` remote before re-pinning
the game (the game fetches the rev from GitHub).
