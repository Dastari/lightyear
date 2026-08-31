use crate::{Link, LinkPlugin, Linked, Linking, RecvLinkConditioner, Unlink, Unlinked};
use alloc::{format, string::String, vec::Vec};
use bevy_app::{App, Plugin};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::{
    relationship::{
        Relationship, RelationshipHookMode, RelationshipSourceCollection, RelationshipTarget,
    },
    world::DeferredWorld,
};
use bevy_reflect::Reflect;
use bevy_utils::prelude::DebugName;
use lightyear_core::time::Instant;
#[allow(unused_imports)]
use tracing::{trace, warn};
// TODO: should we also have a LinkId (remote addr/etc.) that uniquely identifies the link?

#[derive(Component, Default, Debug, Reflect)]
#[component(on_add = Server::on_add)]
#[relationship_target(relationship = LinkOf, linked_spawn)]
pub struct Server {
    #[relationship]
    links: Vec<Entity>,
    /// Receive conditioner cloned into each new [`LinkOf`] child.
    ///
    /// The server endpoint does not receive packets itself. This conditioner is a template; each
    /// child link receives an independent clone whose runtime state lives in [`Link::recv`].
    #[reflect(ignore)]
    pub conditioner: Option<RecvLinkConditioner>,
}

impl Server {
    /// Creates a server with an optional receive conditioner for its child links.
    pub fn new(conditioner: Option<RecvLinkConditioner>) -> Self {
        Self {
            links: Vec::new(),
            conditioner,
        }
    }

    fn on_add(mut world: DeferredWorld, context: HookContext) {
        let entity_ref = world.entity(context.entity);
        if !entity_ref.contains::<Unlinked>()
            && !entity_ref.contains::<Linked>()
            && !entity_ref.contains::<Linking>()
        {
            trace!("Inserting Unlinked because Server was added");
            world.commands().entity(context.entity).insert(Unlinked {
                reason: String::new(),
            });
        };
    }

    fn unlinked(
        trigger: On<Add, Unlinked>,
        mut query: Query<(&Server, &Unlinked)>,
        mut commands: Commands,
    ) {
        if let Ok((server_link, unlinked)) = query.get_mut(trigger.entity) {
            for link_of in server_link.collection() {
                commands.trigger(Unlink {
                    entity: trigger.entity,
                    reason: unlinked.reason.clone(),
                });
                if let Ok(mut c) = commands.get_entity(*link_of) {
                    trace!("d");
                    // cannot simply insert Unlinked because then we wouldn't close aeronet sessions...
                    c.try_despawn();
                }
            }
        }
    }
}

/// Copies a server's receive conditioner into each newly-created client link.
///
/// A server entity is only the listening endpoint; packets are received by its [`LinkOf`] child
/// entities. Keeping the conditioner in [`Link::recv`] lets all IO backends use their existing
/// receive path unchanged.
fn add_server_link_conditioner(
    trigger: On<Add, LinkOf>,
    mut links: Query<(&LinkOf, &mut Link)>,
    servers: Query<&Server>,
) {
    let Ok((link_of, mut link)) = links.get_mut(trigger.entity) else {
        return;
    };
    let Ok(server) = servers.get(link_of.server) else {
        return;
    };
    let Some(conditioner) = &server.conditioner else {
        return;
    };
    if link.recv.conditioner.is_some() {
        return;
    }

    // A backend may queue packets before deferred observers run. Reinsert them so they are
    // conditioned too instead of bypassing the newly inherited template.
    let queued_packets: Vec<_> = link.recv.drain().collect();
    link.recv.conditioner = Some(conditioner.clone());
    for packet in queued_packets {
        link.recv.push(packet, Instant::now());
    }
}

// We add our own relationship hooks instead of deriving relationship
// because we don't want to despawn Server if there are no more LinkOfs.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Reflect)]
#[component(on_insert = LinkOf::on_insert_hook)]
#[component(on_discard = LinkOf::on_replace)]
pub struct LinkOf {
    pub server: Entity,
}

impl Relationship for LinkOf {
    type RelationshipTarget = Server;
    #[inline(always)]
    fn get(&self) -> Entity {
        self.server
    }
    #[inline]
    fn from(entity: Entity) -> Self {
        Self { server: entity }
    }

    fn set_risky(&mut self, entity: Entity) {
        self.server = entity;
    }
}

impl LinkOf {
    fn on_insert_hook(
        mut world: DeferredWorld,
        HookContext {
            entity,
            caller,
            relationship_hook_mode,
            ..
        }: HookContext,
    ) {
        match relationship_hook_mode {
            RelationshipHookMode::Run => {}
            RelationshipHookMode::Skip => return,
            RelationshipHookMode::RunIfNotLinked => return,
        }
        let target_entity = world.entity(entity).get::<Self>().unwrap().get();
        if target_entity == entity {
            warn!(
                "{}The {}({target_entity:?}) relationship on entity {entity:?} points to itself. The invalid {} relationship has been removed.",
                caller
                    .map(|location| format!("{location}: "))
                    .unwrap_or_default(),
                DebugName::type_name::<Self>(),
                DebugName::type_name::<Self>()
            );
            world.commands().entity(entity).remove::<Self>();
            return;
        }
        if let Ok(mut target_entity_mut) = world.get_entity_mut(target_entity) {
            if let Some(mut relationship_target) = target_entity_mut.get_mut::<Server>() {
                relationship_target.collection_mut_risky().add(entity);
            } else {
                let mut target = <Server as RelationshipTarget>::with_capacity(1);
                target.collection_mut_risky().add(entity);
                world.commands().entity(target_entity).insert(target);
            }
        } else {
            warn!(
                "{}The {}({target_entity:?}) relationship on entity {entity:?} relates to an entity that does not exist. The invalid {} relationship has been removed.",
                caller
                    .map(|location| format!("{location}: "))
                    .unwrap_or_default(),
                DebugName::type_name::<Self>(),
                DebugName::type_name::<Self>()
            );
            world.commands().entity(entity).remove::<Self>();
        }
    }

    fn on_replace(
        mut world: DeferredWorld,
        HookContext {
            entity,
            relationship_hook_mode,
            ..
        }: HookContext,
    ) {
        match relationship_hook_mode {
            RelationshipHookMode::Run => {}
            RelationshipHookMode::Skip => return,
            RelationshipHookMode::RunIfNotLinked => {
                if <Server as RelationshipTarget>::LINKED_SPAWN {
                    return;
                }
            }
        }
        let target_entity = world.entity(entity).get::<Self>().unwrap().get();
        if let Ok(mut target_entity_mut) = world.get_entity_mut(target_entity)
            && let Some(mut relationship_target) = target_entity_mut.get_mut::<Server>()
        {
            RelationshipSourceCollection::remove(
                relationship_target.collection_mut_risky(),
                entity,
            );
        }
    }
}

pub struct ServerLinkPlugin;
impl Plugin for ServerLinkPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<LinkPlugin>() {
            app.add_plugins(LinkPlugin);
        }
        app.add_observer(Server::unlinked);
        app.add_observer(add_server_link_conditioner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditioner::LinkConditionerConfig;
    use core::time::Duration;

    #[test]
    fn link_of_inherits_server_conditioner() {
        let mut app = App::new();
        app.add_plugins(ServerLinkPlugin);
        let server = app
            .world_mut()
            .spawn(Server::new(Some(RecvLinkConditioner::new(
                LinkConditionerConfig {
                    incoming_latency: Duration::from_millis(100),
                    ..Default::default()
                },
            ))))
            .id();

        let link = app
            .world_mut()
            .spawn((LinkOf { server }, Link::default()))
            .id();

        assert!(
            app.world()
                .entity(link)
                .get::<Link>()
                .unwrap()
                .recv
                .conditioner
                .is_some()
        );
    }

    #[test]
    fn existing_link_conditioner_is_not_overwritten() {
        let mut app = App::new();
        app.add_plugins(ServerLinkPlugin);
        let server = app
            .world_mut()
            .spawn(Server::new(Some(RecvLinkConditioner::new(
                LinkConditionerConfig {
                    incoming_latency: Duration::from_millis(100),
                    ..Default::default()
                },
            ))))
            .id();
        let link = app
            .world_mut()
            .spawn((
                LinkOf { server },
                Link::new(Some(RecvLinkConditioner::new(LinkConditionerConfig {
                    incoming_loss: 1.0,
                    ..Default::default()
                }))),
            ))
            .id();

        let mut entity = app.world_mut().entity_mut(link);
        let mut link = entity.get_mut::<Link>().unwrap();
        link.recv
            .push(bytes::Bytes::from_static(b"packet"), Instant::now());
        assert_eq!(
            link.recv.conditioner.as_ref().unwrap().time_queue.len(),
            0,
            "the link's loss=1 conditioner must survive server propagation"
        );
    }

    #[test]
    fn queued_packets_are_reinserted_through_the_inherited_conditioner() {
        let mut app = App::new();
        app.add_plugins(ServerLinkPlugin);
        let server = app
            .world_mut()
            .spawn(Server::new(Some(RecvLinkConditioner::new(
                LinkConditionerConfig {
                    incoming_latency: Duration::from_millis(100),
                    ..Default::default()
                },
            ))))
            .id();
        let link = app.world_mut().spawn(Link::default()).id();
        app.world_mut()
            .entity_mut(link)
            .get_mut::<Link>()
            .unwrap()
            .recv
            .push_raw(bytes::Bytes::from_static(b"early"));

        app.world_mut().entity_mut(link).insert(LinkOf { server });

        let link = app.world().entity(link).get::<Link>().unwrap();
        assert_eq!(link.recv.len(), 0, "the raw receive buffer was drained");
        assert_eq!(
            link.recv.conditioner.as_ref().unwrap().time_queue.len(),
            1,
            "the early packet was queued in the inherited conditioner"
        );
    }
}
