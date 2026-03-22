//! Managed the history buffer, which is a buffer of the past predicted component states,
//! so that whenever we receive an update from the server we can compare the predicted entity's history with the server update.
use crate::Predicted;
use crate::rollback::DeterministicPredicted;
use bevy_ecs::prelude::*;
use bevy_utils::prelude::DebugName;
use core::ops::Deref;
use lightyear_core::history_buffer::HistoryBuffer;
#[cfg(test)]
use lightyear_core::history_buffer::HistoryState;
use lightyear_core::prelude::LocalTimeline;
use lightyear_core::timeline::SyncEvent;
use lightyear_replication::prelude::{Confirmed, PreSpawned};
use lightyear_sync::prelude::InputTimelineConfig;
#[allow(unused_imports)]
use tracing::{info, trace};

pub type PredictionHistory<C> = HistoryBuffer<C>;

/// If PredictionMode::Full, we store every update on the predicted entity in the PredictionHistory
///
/// This system only handles changes, removals are handled in `apply_component_removal`
pub(crate) fn update_prediction_history<T: Component + Clone>(
    mut query: Query<(Ref<T>, &mut PredictionHistory<T>)>,
    timeline: Res<LocalTimeline>,
) {
    // tick for which we will record the history (either the current client tick or the current rollback tick)
    let tick = timeline.tick();

    // update history if the predicted component changed
    for (component, mut history) in query.iter_mut() {
        // change detection works even when running the schedule for rollback
        if component.is_changed() {
            // trace!(
            //     "Prediction history changed for tick {tick:?} component {:?}",
            //     DebugName::type_name::<T>()
            // );
            history.add_update(tick, component.deref().clone());
        }
    }
}

/// If there is a TickEvent and the client tick suddenly changes, we need
/// to update the ticks in the history buffer.
///
/// The history buffer ticks are only relevant relative to the current client tick.
/// (i.e. X ticks in the past compared to the current tick)
pub(crate) fn handle_tick_event_prediction_history<C: Component>(
    trigger: On<SyncEvent<InputTimelineConfig>>,
    mut query: Query<&mut PredictionHistory<C>>,
) {
    for mut history in query.iter_mut() {
        trace!(
            "Prediction history updated for {:?} with tick delta {:?}",
            DebugName::type_name::<C>(),
            trigger.tick_delta
        );
        history.update_ticks(trigger.tick_delta);
    }
}

/// If a predicted component is removed on the [`Predicted`] entity, add the removal to the history (for potential rollbacks).
///
/// (if [`Confirmed<C>`] is removed from the component, we don't need to do anything. We might get a rollback
/// by comparing with the history)
pub(crate) fn apply_component_removal_predicted<C: Component>(
    trigger: On<Remove, C>,
    mut predicted_query: Query<&mut PredictionHistory<C>>,
    timeline: Res<LocalTimeline>,
) {
    let tick = timeline.tick();
    // if the component was removed from the Predicted entity, add the Removal to the history
    if let Ok(mut history) = predicted_query.get_mut(trigger.entity) {
        // tick for which we will record the history (either the current client tick or the current rollback tick)
        history.add_remove(tick);
    }
}

/// If a predicted component gets added to [`Predicted`] entity, add a [`PredictionHistory`] component.
///
/// We don't put any value in the history because the `update_history` systems will add the value.
///
/// Predicted: when [`Confirmed<C>`] is added, we potentially do a rollback which will add C
/// PreSpawned:
///   - on the client the component C is added, which should be added to the history
///   - before matching, any rollback should bring us back to the state of C in the history
///   - when Predicted is added (on PreSpawn match), [`Confirmed<C>`] might be added, which shouldn't trigger a rollback
///     because it should match the state of C in the history. We remove PreSpawned to make sure that we rollback to
///     the [`Confirmed<C>`] state
///   - if no match, we also remove PreSpawned, so that the entity is just Predicted (and we rollback to the last [`Confirmed<C>`] state)
pub(crate) fn add_prediction_history<C: Component + Clone>(
    trigger: On<
        Add,
        (
            Confirmed<C>,
            C,
            Predicted,
            PreSpawned,
            DeterministicPredicted,
        ),
    >,
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    // TODO: should we also have With<ShouldBePredicted>?
    query: Query<
        Option<&C>,
        (
            Without<PredictionHistory<C>>,
            Or<(With<Confirmed<C>>, With<C>)>,
            Or<(
                With<Predicted>,
                With<PreSpawned>,
                With<DeterministicPredicted>,
            )>,
        ),
    >,
) {
    if let Ok(component) = query.get(trigger.entity) {
        trace!(
            "Add prediction history for {:?} on entity {:?}",
            DebugName::type_name::<C>(),
            trigger.entity
        );
        let mut history = PredictionHistory::<C>::default();
        if let Some(component) = component {
            history.add_update(timeline.tick(), component.clone());
        }
        commands.entity(trigger.entity).insert(history);
    }
}

/// When [`Predicted`] is inserted on an entity that already has prediction-relevant state, we need
/// to ensure [`PredictionHistory<C>`] exists immediately.
///
/// This mirrors the interpolated late-attach bootstrap path:
/// - `Predicted` first, then `C` / `Confirmed<C>`
/// - `C` / `Confirmed<C>` first, then `Predicted`
///
/// The history is seeded with the current predicted value when `C` already exists so an immediate
/// confirmed update does not spuriously rollback only because the history buffer was still empty.
pub(crate) fn add_prediction_history_on_predicted<C: Component + Clone>(
    trigger: On<Add, Predicted>,
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    query: Query<
        Option<&C>,
        (
            With<Predicted>,
            Without<PredictionHistory<C>>,
            Or<(With<Confirmed<C>>, With<C>)>,
        ),
    >,
) {
    if let Ok(component) = query.get(trigger.entity) {
        trace!(
            "Add prediction history for {:?} on predicted entity {:?}",
            DebugName::type_name::<C>(),
            trigger.entity
        );
        let mut history = PredictionHistory::<C>::default();
        if let Some(component) = component {
            history.add_update(timeline.tick(), component.clone());
        }
        commands.entity(trigger.entity).insert(history);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_app::App;
    use lightyear_core::prelude::Tick;

    #[derive(Component, Clone, PartialEq, Debug)]
    struct TestComp(f32);

    #[test]
    fn seeds_prediction_history_when_component_added_on_predicted_entity() {
        let mut app = App::new();
        let mut timeline = LocalTimeline::default();
        timeline.apply_delta(9);
        app.insert_resource(timeline);
        app.add_observer(add_prediction_history::<TestComp>);
        app.add_observer(add_prediction_history_on_predicted::<TestComp>);

        let entity = app.world_mut().spawn(Predicted).id();
        app.update();
        app.world_mut().entity_mut(entity).insert(TestComp(3.5));
        app.update();

        let history = app
            .world()
            .get::<PredictionHistory<TestComp>>(entity)
            .expect("prediction history should exist");
        assert_eq!(history.len(), 1);
        assert_eq!(
            history.peek(),
            Some(&(Tick(9), HistoryState::Updated(TestComp(3.5))))
        );
    }

    #[test]
    fn seeds_prediction_history_when_predicted_added_after_component_exists() {
        let mut app = App::new();
        let mut timeline = LocalTimeline::default();
        timeline.apply_delta(17);
        app.insert_resource(timeline);
        app.add_observer(add_prediction_history::<TestComp>);
        app.add_observer(add_prediction_history_on_predicted::<TestComp>);

        let entity = app.world_mut().spawn(TestComp(6.25)).id();
        app.update();
        app.world_mut().entity_mut(entity).insert(Predicted);
        app.update();

        let history = app
            .world()
            .get::<PredictionHistory<TestComp>>(entity)
            .expect("prediction history should exist");
        assert_eq!(history.len(), 1);
        assert_eq!(
            history.peek(),
            Some(&(Tick(17), HistoryState::Updated(TestComp(6.25))))
        );
    }
}
