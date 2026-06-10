use alloc::{vec, vec::Vec};
use bevy_platform::collections::HashMap;

use crate::packet::message::{FragmentData, MessageId};
use crate::packet::packet::FRAGMENT_SIZE;
use bytes::Bytes;
use core::time::Duration;
use lightyear_core::tick::Tick;
use tracing::trace;

/// Conservative upper bound on the reassembled size of a single fragmented
/// message. `num_fragments` is supplied by the remote peer on the wire, so
/// without a bound a malformed or malicious packet could declare a huge value
/// and force an unbounded `num_fragments * FRAGMENT_SIZE` allocation — a cheap
/// memory-exhaustion amplification (a few bytes on the wire, megabytes
/// allocated). 4 MiB comfortably exceeds any legitimate lightyear message.
const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

/// Maximum number of fragments accepted for one message, derived from
/// [`MAX_MESSAGE_BYTES`]. Anything larger is dropped before allocating.
const MAX_FRAGMENTS: usize = MAX_MESSAGE_BYTES.div_ceil(FRAGMENT_SIZE);

/// `FragmentReceiver` is used to reconstruct fragmented messages
#[derive(Debug)]
pub struct FragmentReceiver {
    fragment_messages: HashMap<MessageId, FragmentConstructor>,
}

impl FragmentReceiver {
    pub fn new() -> Self {
        Self {
            fragment_messages: HashMap::default(),
        }
    }

    /// Discard all messages for which the latest fragment was received before the cleanup time
    /// (i.e. we probably lost some fragments and we will never complete the message)
    ///
    /// If we don't keep track of the last received time, we will never clean up the messages.
    pub fn cleanup(&mut self, cleanup_time: Duration) {
        self.fragment_messages.retain(|_, c| {
            c.last_received
                .map(|t| t > cleanup_time)
                .unwrap_or_else(|| true)
        })
    }

    /// Receive a fragment of a FragmentData message.
    ///
    /// When we complete the final message by aggregating all fragments, we will return the
    /// `remote_sent_tick` associated with the first fragment received.
    pub fn receive_fragment(
        &mut self,
        fragment: FragmentData,
        remote_sent_tick: Tick,
        current_time: Option<Duration>,
    ) -> Option<(Tick, Bytes)> {
        // `num_fragments` and `fragment_id` are attacker-controlled values read
        // straight off the wire. Validate them before allocating or indexing so
        // a malformed packet is dropped rather than panicking the whole app —
        // the transport runs below application auth, so this path is reachable
        // by any peer that can send us a packet.
        let num_fragments = fragment.num_fragments.0;
        let fragment_id = fragment.fragment_id.0;
        if num_fragments == 0
            || num_fragments > MAX_FRAGMENTS as u64
            || fragment_id >= num_fragments
        {
            trace!(
                message_id = ?fragment.message_id,
                num_fragments,
                fragment_id,
                "dropping fragment with invalid metadata"
            );
            return None;
        }
        let num_fragments = num_fragments as usize;

        let fragment_message = self
            .fragment_messages
            .entry(fragment.message_id)
            .or_insert_with(|| FragmentConstructor::new(remote_sent_tick, num_fragments));

        // A later fragment for the same message id may disagree with the first
        // about the total count; reject anything inconsistent with the buffer we
        // already sized, otherwise the index/size checks below could be bypassed.
        if fragment_message.num_fragments != num_fragments {
            trace!(
                message_id = ?fragment.message_id,
                expected = fragment_message.num_fragments,
                got = num_fragments,
                "dropping fragment with mismatched num_fragments"
            );
            return None;
        }

        // completed the fragmented message!
        if let Some(payload) = fragment_message.receive_fragment(
            fragment_id as usize,
            fragment.bytes.as_ref(),
            current_time,
        ) {
            self.fragment_messages.remove(&fragment.message_id);
            return Some(payload);
        }

        None
    }
}

#[derive(Debug, Clone)]
/// Data structure to reconstruct a single fragmented message from individual fragments
pub struct FragmentConstructor {
    num_fragments: usize,
    num_received_fragments: usize,
    received: Vec<bool>,
    // bytes: Bytes,
    bytes: Vec<u8>,

    tick: Tick,
    last_received: Option<Duration>,
}

impl FragmentConstructor {
    pub fn new(tick: Tick, num_fragments: usize) -> Self {
        Self {
            num_fragments,
            num_received_fragments: 0,
            received: vec![false; num_fragments],
            bytes: vec![0; num_fragments * FRAGMENT_SIZE],
            tick,
            last_received: None,
        }
    }

    pub fn receive_fragment(
        &mut self,
        fragment_index: usize,
        bytes: &[u8],
        received_time: Option<Duration>,
    ) -> Option<(Tick, Bytes)> {
        self.last_received = received_time;

        // Defensive bounds: `FragmentReceiver::receive_fragment` validates these
        // before calling us, but never index or copy outside the reassembly
        // buffer even if invoked directly with bad input — drop instead. This
        // also keeps `num_fragments - 1` below from underflowing when
        // `num_fragments == 0` (only reachable if num_fragments >= 1).
        if fragment_index >= self.num_fragments {
            return None;
        }
        let is_last_fragment = fragment_index == self.num_fragments - 1;

        // Each non-final fragment must be exactly FRAGMENT_SIZE; the final one
        // may be smaller. Reject anything that would overflow the buffer.
        if bytes.len() > FRAGMENT_SIZE || (!is_last_fragment && bytes.len() != FRAGMENT_SIZE) {
            return None;
        }

        if !self.received[fragment_index] {
            self.received[fragment_index] = true;
            self.num_received_fragments += 1;

            if is_last_fragment {
                let len = (self.num_fragments - 1) * FRAGMENT_SIZE + bytes.len();
                self.bytes.resize(len, 0);
            }

            let start = fragment_index * FRAGMENT_SIZE;
            let end = start + bytes.len();
            self.bytes[start..end].copy_from_slice(bytes);
        }

        if self.num_received_fragments == self.num_fragments {
            trace!("Received all fragments!");
            let payload = core::mem::take(&mut self.bytes);
            return Some((self.tick, payload.into()));
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use crate::channel::senders::fragment_sender::FragmentSender;
    use crate::packet::message::FragmentIndex;

    use super::*;

    /// Build a single raw `FragmentData` with arbitrary (possibly invalid)
    /// metadata, as a hostile peer could put on the wire.
    fn fragment(fragment_id: u64, num_fragments: u64, len: usize) -> FragmentData {
        FragmentData {
            message_id: MessageId(0),
            fragment_id: FragmentIndex(fragment_id),
            num_fragments: FragmentIndex(num_fragments),
            bytes: Bytes::from(vec![1u8; len]),
        }
    }

    #[test]
    fn test_receiver() {
        let mut receiver = FragmentReceiver::new();
        let num_bytes = (FRAGMENT_SIZE as f32 * 1.5) as usize;
        let message_bytes = Bytes::from(vec![1u8; num_bytes]);
        let fragments = FragmentSender::new().build_fragments(MessageId(0), message_bytes.clone());

        assert_eq!(
            receiver.receive_fragment(fragments[0].clone(), Tick(0), None),
            None
        );
        assert_eq!(
            receiver.receive_fragment(fragments[1].clone(), Tick(1), None),
            Some((Tick(0), message_bytes.clone()))
        );
    }

    // The following all reproduce inputs that previously panicked (or would
    // allocate unboundedly) in the fragment reassembler. Each must now be
    // dropped as `None` without panicking.

    #[test]
    fn zero_num_fragments_is_dropped() {
        let mut receiver = FragmentReceiver::new();
        // `num_fragments == 0` underflowed `self.num_fragments - 1` and panicked
        // the whole app (the original crash).
        assert_eq!(
            receiver.receive_fragment(fragment(0, 0, 8), Tick(0), None),
            None
        );
    }

    #[test]
    fn fragment_id_out_of_range_is_dropped() {
        let mut receiver = FragmentReceiver::new();
        // `fragment_id >= num_fragments` indexed past `received`/the buffer.
        assert_eq!(
            receiver.receive_fragment(fragment(5, 3, FRAGMENT_SIZE), Tick(0), None),
            None
        );
    }

    #[test]
    fn excessive_num_fragments_is_dropped_before_allocating() {
        let mut receiver = FragmentReceiver::new();
        // A few bytes on the wire must not force a multi-GiB allocation.
        assert_eq!(
            receiver.receive_fragment(fragment(0, u64::MAX, 8), Tick(0), None),
            None
        );
        assert_eq!(
            receiver.receive_fragment(fragment(0, MAX_FRAGMENTS as u64 + 1, 8), Tick(0), None),
            None
        );
    }

    #[test]
    fn oversized_fragment_payload_is_dropped() {
        let mut receiver = FragmentReceiver::new();
        // Non-final fragment larger than FRAGMENT_SIZE would overflow the buffer.
        assert_eq!(
            receiver.receive_fragment(fragment(0, 3, FRAGMENT_SIZE + 1), Tick(0), None),
            None
        );
    }

    #[test]
    fn mismatched_num_fragments_for_same_message_is_dropped() {
        let mut receiver = FragmentReceiver::new();
        assert_eq!(
            receiver.receive_fragment(fragment(0, 3, FRAGMENT_SIZE), Tick(0), None),
            None
        );
        // A second packet for the same message id claiming a different total
        // must not be applied against the already-sized buffer.
        assert_eq!(
            receiver.receive_fragment(fragment(1, 4, FRAGMENT_SIZE), Tick(0), None),
            None
        );
    }
}
