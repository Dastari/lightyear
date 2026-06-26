# Lightyear (fork)

A library for writing server-authoritative multiplayer games with [Bevy](https://bevyengine.org/). Compatible with wasm
via WebTransport.

> ### Fork notice
>
> This is a **hard fork** of [`cBournhonesque/lightyear`](https://github.com/cBournhonesque/lightyear),
> **permanently diverged** from the upstream `0.26.4` line. It deliberately **keeps the original custom
> replication backend** — it does **not** adopt the `bevy_replicon` rewrite that upstream shipped in
> `0.27` — and retains the flat workspace layout. On top of that base it adds independent Bevy-version
> support plus extra hardening and features (see [Fork additions](#fork-additions)).
>
> **✅ Bevy 0.19-ready** — this fork targets **Bevy 0.19** (`avian` 0.7, `aeronet` 0.21, MSRV 1.95).

https://github.com/cBournhonesque/lightyear/assets/8112632/7b57d48a-d8b0-4cdd-a16f-f991a394c852

*Demo using one server with 2 clients. The entity is predicted (slightly ahead of server) on the controlling client and
interpolated (slightly behind server) on the other client.
The server only sends updates to clients 10 times per second but the clients still see smooth updates.*

## Fork extensions

This fork is **upstream-compatible by default**: a default build behaves like upstream `0.26.4` plus
always-on security/correctness hardening. Every *opinionated* behavior change is **opt-in**, so other
projects get vanilla behavior unless they ask for more. Targets **Bevy 0.19** (`avian` 0.7,
`aeronet` 0.21, MSRV 1.95).

### Opt-in behaviors (default = upstream)

| Extension | Default | Enable |
| --- | --- | --- |
| Deeper input-history retention (rollback reach) | 20 ticks | `InputConfig { history_depth, .. }` (per `Action`) |
| Convergent interpolation (smart-drain + idle-rebase + clamp-at-newest, vs. extrapolate) | off | `InterpolationConfig::default().with_convergent_history(true)` on the client |
| Input target authorization (drop forged `InputTarget::Entity`) | off | `app.add_input_validator(authorize_controlled_targets::<S>)` |
| Late-attach init (seed prediction/interpolation history + bootstrap avian `Transform` on out-of-order lane adoption) | off | `app.insert_resource(ForkExtensions::all())` |

`InputSystems::ValidateInputs` + `add_input_validator` are also the general seam for game-side input
validation — drop/clamp/authorize received messages (via `MessageReceiver::retain_messages`) before
they are buffered.

### Always-on (security & correctness — not opt-in)

- **Wire-decode / DoS hardening:** bounded length-prefix decode (anti-OOM) in `serde`/`replication`,
  fragment-metadata validation, `FRAGMENT_SIZE` from worst-case varint sizes, inbound decode-reject
  metering, and the input `end_tick` lookahead bound (protects `InputBuffer` from OOM).
- **Correctness:** delta-ack monotonicity, base-diff fallback when an ack base is missing, panic-safe
  (`try_`) interpolation commands.
- **Resilience:** UDP send-preservation under backpressure, peer-address eviction on `LinkOf` unlink.

### Already opt-in (upstream-compatible defaults)

- Delta keyframes + Avian2D velocity diffs (`add_delta_compression_with_keyframe_interval`).
- Replication send-metrics observer (`ReplicationSendMetricsObserver`).
- WebTransport DNS host (`WebTransportClientIo.server_host`).
- f64 Avian2D visual correction (the `f64` Cargo feature).
- Native input-state sequence visibility; prepare-input send-rate via `InputConfig::send_interval`.

## Getting started 

You can first check out the [examples](https://github.com/cBournhonesque/lightyear/tree/main/examples).

To quickly get started, you can follow
this [tutorial](https://cbournhonesque.github.io/lightyear/book/tutorial/title.html), which re-creates
the [simple_box](https://github.com/cBournhonesque/lightyear/tree/main/examples/simple_box) example.

You can also find more information in this WIP [book](https://cbournhonesque.github.io/lightyear/book/).

## Related projects

- [lightyear-template](https://github.com/Piefayth/lightyear-template/tree/main): opiniated template for a bevy + lightyear starter project

### Games

- [Lumina](https://github.com/nixon-voxell/lumina)
- [cycles.io](https://github.com/cBournhonesque/jam5) for bevy jam 5: https://cbournhonesque.itch.io/cyclesio


## Features


- Transport-agnostic: *Lightyear* is compatible with a number of IO backends, including:
    - UDP sockets
    - WebTransport (using QUIC): available on both native and wasm!
    - WebSocket: available on both native and wasm!
    - Steam: use the SteamWorks SDK to send messages over the Steam network
- Serialization
    - *Lightyear* uses `bincode` as a default serializer, but you can provide your own serialization function
- Message passing
    - *Lightyear* supports sending packets with different guarantees of ordering and reliability through the use of
      channels.
    - Packet fragmentation (for messages larger than ~1200 bytes) is supported
- Input handling
    - *Lightyear* has special handling for player inputs (mouse presses, keyboards).
      They are buffered every tick on the `Client`, and *lightyear* makes sure that the client input for tick `N` will
      be processed on tick `N` on the server.
      Inputs are protected against packet-loss: each packet will contain the client inputs for the last few frames.
    - With the `leafwing` feature, there is a special integration with
      the [`leafwing-input-manager`](https://github.com/Leafwing-Studios/leafwing-input-manager) crate, where
      your `leafwing` inputs are networked for you!
    - Also supports the [`bevy-enhanced-input`](https://github.com/projectharmonia/bevy_enhanced_input) crate!
- Deterministic replication
    - *Lightyear* supports deterministic replication when only inputs are replicated. The simulation needs to be deterministic.
      The deterministic replication is compatible with both lockstep and prediction/rollback.
- World Replication
    - Entities that have the `Replicate` bundle will be automatically replicated to clients.
- Advanced replication
    - **Client-side prediction**: with just a one-line change, you can enable client-prediction with rollback on the
      client, so that your inputs can feel responsive
    - **Snapshot interpolation**: with just a one-line change, you can enable Snapshot interpolation so that entities
      are smoothly interpolated even if replicated infrequently.
    - **Client-authoritative replication**: you can also replicate entities from the client to the server. The authority over the entity is transferable between the client and the server.
    - **Pre-spawning predicted entities**: you can spawn Predicted entities on the client first, and then transfer the
      authority to the server. This ensures that the entity is spawned immediately, but will still be controlled by the server.
    - **Entity mapping**: *lightyear* also supports replicating components/messages that contain references to other
      entities. The entities will be mapped from the local World to the remote World.
    - **Interest management**: *lightyear* supports replicating only a subset of the World to clients. Interest
      management is made flexible by the use of `Rooms`
    - **Input Delay**: you can add a custom amount of input-delay as a trade-off between having a more responsive game
      or more miss-predictions
    - **Bandwidth Management**: you can set a cap to the bandwidth for the connection. Then messages will be sent in
      decreasing order of priority (that you can set yourself), with a priority-accumulation scheme
    - **Lag Compensation** is available so that predicted entities can interact with interpolated entities (used most often for fps games)
- Configurable
    - *Lightyear* is highly configurable: you can configure the size of the input buffer, the amount of
      interpolation-delay, the packet send rate, etc.
- Observability
    - *Lightyear* uses the `tracing` and `metrics` libraries to emit spans and logs around most events (
      sending/receiving messages, etc.).
- Examples
    - *Lightyear* has plenty of examples demonstrating all these features, as well as the integration with other bevy
      crates such as `avian`



## Supported bevy version

| Lightyear           | Bevy |
|---------------------|------|
| this fork (`main`)  | 0.19 |
| 0.26                | 0.18 |
| 0.25      | 0.17 |
| 0.20-0.24 | 0.16 |
| 0.18-0.19 | 0.15 |
| 0.16-0.17 | 0.14 |
| 0.10-0.15 | 0.13 |
| 0.1-0.9   | 0.12 |
