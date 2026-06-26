//! Global opt-in switches for this fork's behavioral extensions.
//!
//! Defaults are all `false` (upstream behavior). Insert [`ForkExtensions`] as a resource — directly
//! or via the fork-extensions preset — to turn extensions on. Systems/observers read it as
//! `Option<Res<ForkExtensions>>`, so an absent resource means "all off".

use bevy_ecs::resource::Resource;

/// Global, runtime opt-in flags for fork-specific behavioral extensions that aren't tied to a
/// per-entity config (e.g. cross-subsystem late-attach handling).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ForkExtensions {
    /// Seed prediction/interpolation history when `Predicted` / `Interpolated` are added *late*
    /// (after the component / `Confirmed`), rather than only on the canonical insertion order.
    ///
    /// Off (upstream): a late lane adoption starts from an empty history. On: the history is
    /// initialized/seeded so the entity doesn't roll back to or interpolate from nothing.
    pub late_attach_init: bool,
}

impl ForkExtensions {
    /// Every fork extension enabled.
    pub fn all() -> Self {
        Self {
            late_attach_init: true,
        }
    }
}

/// Convenience for systems taking `Option<Res<ForkExtensions>>`: is `late_attach_init` enabled?
/// (Absent resource ⇒ `false`.) Call as `late_attach_init_enabled(fork.as_deref())`.
pub fn late_attach_init_enabled(fork: Option<&ForkExtensions>) -> bool {
    fork.is_some_and(|f| f.late_attach_init)
}
