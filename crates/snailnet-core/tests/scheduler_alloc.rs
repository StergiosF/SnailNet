//! The scheduler's packet path doesn't allocate once warmed up (PLAN.md
//! §20), including the per-app pressure counting: CoDel drops, hard limit
//! drops and flow queues changing owner. A counting allocator, in its own
//! test binary so no other test shares it, counts this thread's
//! allocations.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use snailnet_core::scheduler::{
    ClassConfig, Dequeue, Packet, PacketMeta, Pressure, Scheduler, SchedulerConfig, Tag,
};

struct Counting;

thread_local! {
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
}

// SAFETY: every call is passed straight to `System`, which upholds the
// `GlobalAlloc` contract; the counter is a const-initialised thread local,
// so touching it never allocates or recurses.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.with(|n| n.set(n.get() + 1));
        // SAFETY: the caller's contract for `alloc` is passed on unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `System.alloc` above with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.with(|n| n.set(n.get() + 1));
        // SAFETY: the caller's contract for `realloc` is passed on unchanged.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocs() -> u64 {
    ALLOCS.with(Cell::get)
}

struct P {
    len: usize,
    tag: Tag,
}

impl Packet for P {
    fn wire_len(&self) -> usize {
        self.len
    }
    fn tag(&self) -> Tag {
        self.tag
    }
}

const MS: u64 = 1_000_000;

/// One second of traffic: two classes; in each, bulk flows arriving at
/// twice what the class can send (CoDel and the queue limit drop), and
/// light flows whose buckets go idle and come back under another owner.
fn round(s: &mut Scheduler<P>, start: u64, generation: u32) -> u64 {
    let mut sent = 0;
    for ms in 0..1000u64 {
        let now = start + ms * MS;
        for class in 0..2 {
            for flow in 0..4u64 {
                for _ in 0..2 {
                    let meta = PacketMeta::new(class, flow + 10 * class as u64);
                    s.enqueue(
                        now,
                        meta,
                        P {
                            len: 1500,
                            tag: flow as Tag,
                        },
                    );
                }
            }
            if ms % 50 == 0 {
                // A short-lived flow: same bucket, a new owner each time.
                let tag = 100 + generation * 7 + (ms / 50) as Tag % 5;
                s.enqueue(now, PacketMeta::new(class, 999), P { len: 200, tag });
            }
        }
        while let Dequeue::Packet { .. } = s.dequeue(now) {
            sent += 1;
        }
    }
    sent
}

#[test]
fn packet_path_and_pressure_do_not_allocate_once_warm() {
    let config = SchedulerConfig {
        rate_bytes_per_sec: 4_000_000, // ~2.7 packets per ms
        burst_bytes: 2 * 1514,
        classes: vec![ClassConfig::new("a", 5000), ClassConfig::new("other", 5000)],
    };
    let mut s: Scheduler<P> = Scheduler::new(config, 0).expect("valid config");
    let mut out: Vec<(Tag, Pressure)> = Vec::with_capacity(256);

    // Warm up: buckets, queues and the retired table reach their sizes.
    for g in 0..3 {
        round(&mut s, g as u64 * 1000 * MS, g);
    }
    s.pressure(&mut out);
    let acted_before: u64 = out.iter().map(|(_, p)| p.acted).sum();

    let before = allocs();
    let mut sent = 0;
    for g in 3..6 {
        sent += round(&mut s, g as u64 * 1000 * MS, g);
        s.pressure(&mut out);
    }
    let after = allocs();

    assert!(sent > 5_000, "sent {sent}");
    let acted: u64 = out.iter().map(|(_, p)| p.acted).sum();
    assert!(
        acted > acted_before + 1000,
        "CoDel acted {acted_before} → {acted}"
    );
    assert_eq!(after - before, 0, "the packet path allocated");
}
