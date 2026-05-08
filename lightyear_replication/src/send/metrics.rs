use crate::send::components::ReplicationGroupId;
use alloc::boxed::Box;
use bevy_ecs::{entity::Entity, resource::Resource};
use lightyear_core::tick::Tick;
use lightyear_transport::packet::message::MessageId;

/// Replication channel class used by the send metrics observer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplicationSendChannel {
    /// Entity spawn/despawn, component inserts/removes, and updates that are coupled to actions.
    Actions,
    /// Standalone component update messages.
    Updates,
}

/// Where a replication message is in the outbound send path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplicationSendStatus {
    /// The serialized replication payload was queued into the transport channel sender.
    Queued,
    /// The transport priority manager selected the queued payload for packet building.
    Sent,
}

/// Compact per-message replication send metrics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplicationSendMetrics {
    /// Entity that owns the [`ReplicationSender`](crate::send::sender::ReplicationSender).
    pub sender_entity: Entity,
    /// Simulation tick when the payload was buffered by replication.
    pub tick: Tick,
    /// Replication group carried by the payload.
    pub group_id: ReplicationGroupId,
    /// Replication transport channel class.
    pub channel: ReplicationSendChannel,
    /// Whether this is queue-time or selected-for-send observability.
    pub status: ReplicationSendStatus,
    /// Transport-level message id when the channel assigns one.
    pub message_id: Option<MessageId>,
    /// Serialized payload bytes queued or selected for sending.
    pub bytes: usize,
    /// Number of replication messages represented by this metric.
    pub message_count: usize,
    /// Number of entities encoded in the replication message.
    pub entity_count: usize,
    /// Number of component payloads encoded in the replication message.
    pub component_count: usize,
    /// Current channel sender queue depth after this observation point.
    pub queue_depth: usize,
    /// True when transport bandwidth limiting can defer this payload.
    pub bandwidth_limited: bool,
}

/// Optional sink for server/application-owned replication send observability.
///
/// Install it with [`ReplicationSendMetricsObserver`] to keep Lightyear free of
/// application-specific metric names.
pub trait ReplicationSendMetricsSink: Send + Sync + 'static {
    fn observe(&self, metrics: ReplicationSendMetrics);
}

/// Resource wrapper for an optional replication metrics sink.
#[derive(Resource)]
pub struct ReplicationSendMetricsObserver {
    sink: Box<dyn ReplicationSendMetricsSink>,
}

impl ReplicationSendMetricsObserver {
    pub fn new(sink: impl ReplicationSendMetricsSink) -> Self {
        Self {
            sink: Box::new(sink),
        }
    }

    #[inline]
    pub(crate) fn observe(&self, metrics: ReplicationSendMetrics) {
        self.sink.observe(metrics);
    }
}
