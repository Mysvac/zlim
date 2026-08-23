//! Integration tests for the message pipeline.

use zlim_core::message::Message;
use zlim_core::message::MessageCursor;
use zlim_core::message::MessageMutator;
use zlim_core::message::MessageQueue;
use zlim_core::message::MessageReader;
use zlim_core::message::MessageWriter;
use zlim_core::system::{IntoSystem, System};
use zlim_core::world::World;
use zlim_reflect::TypePath;

// -----------------------------------------------------------------------------
// Message types

#[derive(TypePath, Message, Clone, Copy, Debug, PartialEq)]
struct Payload(u32);

#[derive(TypePath, Message, Clone, Copy, Debug, PartialEq)]
struct Other(u32);

#[derive(TypePath, Message, Clone, Debug, PartialEq)]
struct GenericMsg<T: Send + Sync + 'static>(T);

// -----------------------------------------------------------------------------
// Queue lifecycle

#[test]
fn queue_write_read_update_lifecycle() {
    let mut queue = MessageQueue::<Payload>::default();
    assert!(queue.is_empty());

    let key = queue.write(Payload(1));
    assert_eq!(key.index(), 0);
    queue.write(Payload(2));
    assert_eq!(queue.len(), 2);
    assert!(!queue.is_empty());
    assert_eq!(queue.oldest_message_index(), 0);

    assert_eq!(queue.get(0).map(|(_, m)| m.0), Some(1));
    assert_eq!(queue.get(1).map(|(_, m)| m.0), Some(2));
    assert_eq!(queue.get(2), None);
    assert_eq!(queue.get_mut(1).map(|(_, m)| m.0), Some(2));

    // First update: messages stay readable for one extra update.
    queue.update();
    assert_eq!(queue.len(), 2);
    queue.write(Payload(3));
    assert_eq!(queue.len(), 3);

    // Second update: the first two messages expire.
    queue.update();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.get(0), None);
    assert_eq!(queue.get(2).map(|(_, m)| m.0), Some(3));
}

#[test]
fn queue_write_batch_yields_key_range() {
    let mut queue = MessageQueue::<Payload>::default();

    let keys: Vec<usize> = queue
        .write_batch([Payload(10), Payload(20), Payload(30)])
        .map(|key| key.index())
        .collect();
    assert_eq!(keys, vec![0, 1, 2]);
    assert_eq!(queue.len(), 3);
    assert_eq!(queue.write_batch([]).count(), 0);
}

#[test]
fn queue_drain_and_update_drain() {
    let mut queue = MessageQueue::<Payload>::default();
    queue.write(Payload(1));
    queue.update();
    queue.write(Payload(2));

    // `update_drain` rotates and drains the expired sequence.
    let stale: Vec<u32> = queue.update_drain().map(|m| m.0).collect();
    assert_eq!(stale, vec![1]);
    assert_eq!(queue.len(), 1);

    // `drain` clears everything currently readable.
    let rest: Vec<u32> = queue.drain().map(|m| m.0).collect();
    assert_eq!(rest, vec![2]);
    assert!(queue.is_empty());
}

// -----------------------------------------------------------------------------
// Cursors

#[test]
fn cursors_are_independent() {
    let mut queue = MessageQueue::<Payload>::default();
    queue.write(Payload(1));
    queue.write(Payload(2));

    let mut c1 = MessageCursor::new(&queue);
    let mut c2 = MessageCursor::new(&queue);
    assert_eq!(c1.len(&queue), 2);
    assert_eq!(c2.len(&queue), 2);

    // `c1` consumes everything; `c2` is unaffected.
    let read: Vec<u32> = c1.read(&queue).map(|m| m.0).collect();
    assert_eq!(read, vec![1, 2]);
    assert!(c1.is_empty(&queue));
    assert_eq!(c1.read(&queue).count(), 0);
    assert_eq!(c2.len(&queue), 2);

    // `c2` reads with ids.
    let with_id: Vec<(usize, u32)> = c2
        .read_with_key(&queue)
        .map(|(key, m)| (key.index(), m.0))
        .collect();
    assert_eq!(with_id, vec![(0, 1), (1, 2)]);
}

#[test]
fn cursor_read_mut_and_clear() {
    let mut queue = MessageQueue::<Payload>::default();
    queue.write(Payload(5));

    let mut cursor = MessageCursor::new(&queue);
    for m in cursor.read_mut(&mut queue) {
        m.0 *= 2;
    }
    assert_eq!(queue.get(0).map(|(_, m)| m.0), Some(10));

    // `clear` consumes without iterating.
    queue.write(Payload(7));
    assert_eq!(cursor.len(&queue), 1);
    cursor.clear(&queue);
    assert!(cursor.is_empty(&queue));
}

// -----------------------------------------------------------------------------
// World integration

#[test]
fn world_register_and_write() {
    let mut world = World::alloc();

    let id_a = world.register_message::<Payload>();
    let id_b = world.register_message::<Other>();
    assert_ne!(id_a, id_b);

    // Registration is idempotent.
    assert_eq!(world.register_message::<Payload>(), id_a);
    assert_eq!(world.messages().len(), 2);
    assert!(world.messages().get(id_a).is_some());
    assert!(world.messages().get_name(id_a).is_some());
    assert!(world.messages().iter().any(|meta| meta.id() == id_b));
    assert!(world.contains_resource::<MessageQueue<Payload>>());

    // Messages written before rotation are readable immediately.
    let key = world.write_message(Payload(42)).unwrap();
    assert_eq!(key.index(), 0);
    assert_eq!(world.resource::<MessageQueue<Payload>>().len(), 1);

    let keys: Vec<usize> = world
        .write_message_batch([Payload(43), Payload(44)])
        .unwrap()
        .map(|key| key.index())
        .collect();
    assert_eq!(keys, vec![1, 2]);

    // One update keeps them readable; the next expires them.
    world.update_messages();
    assert_eq!(world.resource::<MessageQueue<Payload>>().len(), 3);
    world.update_messages();
    assert!(world.resource::<MessageQueue<Payload>>().is_empty());
}

#[test]
fn write_unregistered_message_fails_softly() {
    let mut world = World::alloc();

    assert!(world.write_message::<Payload>(Payload(1)).is_none());
    assert!(
        world
            .write_message_batch::<Payload>([Payload(1), Payload(2)])
            .is_none()
    );
}

#[test]
fn generic_message_type() {
    let mut queue = MessageQueue::<GenericMsg<u32>>::default();
    queue.write(GenericMsg(7));
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.get(0).map(|(_, m)| m.0), Some(7));
}

// -----------------------------------------------------------------------------
// System parameters

fn write_messages(mut writer: MessageWriter<Payload>) {
    writer.write(Payload(10));
    writer.write(Payload(20));
}

fn clamp_messages(mut mutator: MessageMutator<Payload>) {
    for m in mutator.read() {
        m.0 = m.0.min(15);
    }
}

fn sum_messages(mut reader: MessageReader<Payload>) -> u32 {
    reader.read().map(|m| m.0).sum()
}

#[test]
fn system_params_end_to_end() {
    let mut world = World::alloc();
    world.register_message::<Payload>();

    let mut writer = IntoSystem::into_system(write_messages);
    writer.initialize(&world);
    let mut mutator = IntoSystem::into_system(clamp_messages);
    mutator.initialize(&world);
    let mut reader = IntoSystem::into_system(sum_messages);
    reader.initialize(&world);

    writer.run((), &mut world).unwrap();

    // Cursors created before the writes still see the new messages.
    mutator.run((), &mut world).unwrap();
    assert_eq!(
        world
            .resource::<MessageQueue<Payload>>()
            .get(0)
            .map(|(_, m)| m.0),
        Some(10)
    );
    assert_eq!(
        world
            .resource::<MessageQueue<Payload>>()
            .get(1)
            .map(|(_, m)| m.0),
        Some(15)
    );

    assert_eq!(reader.run((), &mut world).unwrap(), 25);

    // The reader cursor has consumed everything.
    assert_eq!(reader.run((), &mut world).unwrap(), 0);

    // New writes in the same frame are visible to the reader cursor.
    world.write_message(Payload(5));
    assert_eq!(reader.run((), &mut world).unwrap(), 5);
}

// -----------------------------------------------------------------------------
