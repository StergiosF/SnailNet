//! Pre-allocated packet buffers (PLAN.md §4: the fast path never allocates
//! per packet).
//!
//! Two shelves of fixed-size buffers: small ones for ordinary packets (MTU
//! 1500 plus headroom) and a few large ones for offloaded (LSO/GRO) packets
//! up to `WINDIVERT_MTU_MAX`. A [`PacketBuf`] goes back on its shelf when
//! dropped, so a buffer can't leak and the pool never grows. When a shelf is
//! empty, [`PacketPool::alloc`] returns `None` and the caller passes the
//! packet straight through (fail open) rather than allocating.

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender, bounded};

use crate::batch::MTU_MAX;

/// Small-buffer size: a full 1500-byte packet with room to spare.
pub const SMALL_SLOT: usize = 2048;
/// Large-buffer size: the largest packet WinDivert can hand us.
pub const LARGE_SLOT: usize = MTU_MAX;

struct Shelf {
    size: usize,
    count: usize,
    put: Sender<Box<[u8]>>,
    take: Receiver<Box<[u8]>>,
}

impl Shelf {
    fn new(size: usize, count: usize) -> Self {
        // A lock-free bounded MPMC queue: rx threads take, the scheduler
        // thread (and anyone dropping a packet) puts back.
        let (put, take) = bounded(count.max(1));
        for _ in 0..count {
            put.try_send(vec![0; size].into_boxed_slice())
                .expect("the shelf has room for every buffer");
        }
        Self {
            size,
            count,
            put,
            take,
        }
    }
}

struct Shelves {
    small: Shelf,
    large: Shelf,
}

/// A cheap-to-clone handle to the pool.
#[derive(Clone)]
pub struct PacketPool {
    shelves: Arc<Shelves>,
}

impl PacketPool {
    /// Allocate every buffer now: `small` × [`SMALL_SLOT`] bytes plus
    /// `large` × [`LARGE_SLOT`] bytes.
    pub fn new(small: usize, large: usize) -> Self {
        Self {
            shelves: Arc::new(Shelves {
                small: Shelf::new(SMALL_SLOT, small),
                large: Shelf::new(LARGE_SLOT, large),
            }),
        }
    }

    /// Copy `data` into a free buffer. `None` if the packet is too big for
    /// any buffer or every buffer of the right size is in use.
    pub fn alloc(&self, data: &[u8]) -> Option<PacketBuf> {
        let large = data.len() > SMALL_SLOT;
        let shelf = if large {
            &self.shelves.large
        } else {
            &self.shelves.small
        };
        if data.len() > shelf.size {
            return None;
        }
        let mut buf = shelf.take.try_recv().ok()?;
        buf[..data.len()].copy_from_slice(data);
        Some(PacketBuf {
            buf: Some(buf),
            len: data.len(),
            large,
            shelves: self.shelves.clone(),
        })
    }

    /// Free buffers right now: (small, large).
    pub fn available(&self) -> (usize, usize) {
        (self.shelves.small.take.len(), self.shelves.large.take.len())
    }

    /// Buffers owned by the pool: (small, large).
    pub fn capacity(&self) -> (usize, usize) {
        (self.shelves.small.count, self.shelves.large.count)
    }
}

/// One packet in a pooled buffer. Derefs to exactly the packet's bytes.
pub struct PacketBuf {
    /// Always `Some` until dropped.
    buf: Option<Box<[u8]>>,
    len: usize,
    large: bool,
    shelves: Arc<Shelves>,
}

impl Deref for PacketBuf {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.buf.as_ref().expect("present until drop")[..self.len]
    }
}

impl DerefMut for PacketBuf {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.buf.as_mut().expect("present until drop")[..self.len]
    }
}

impl Drop for PacketBuf {
    fn drop(&mut self) {
        let shelf = if self.large {
            &self.shelves.large
        } else {
            &self.shelves.small
        };
        if let Some(buf) = self.buf.take() {
            // Never full: only buffers taken from this shelf come back.
            let _ = shelf.put.try_send(buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffers_come_back_and_are_reused() {
        let pool = PacketPool::new(2, 1);
        let a = pool.alloc(&[1, 2, 3]).unwrap();
        let ptr = a.as_ptr();
        assert_eq!(&*a, &[1, 2, 3]);
        assert_eq!(pool.available(), (1, 1));
        drop(a);
        assert_eq!(pool.available(), (2, 1));
        // FIFO shelf: take the other one, then ours comes round again.
        let b = pool.alloc(&[9]).unwrap();
        let c = pool.alloc(&[8; 100]).unwrap();
        assert!(b.as_ptr() == ptr || c.as_ptr() == ptr);
        assert_eq!(c.len(), 100);
    }

    #[test]
    fn exhaustion_returns_none_instead_of_allocating() {
        let pool = PacketPool::new(1, 0);
        let held = pool.alloc(&[0; 60]).unwrap();
        assert!(pool.alloc(&[0; 60]).is_none());
        drop(held);
        assert!(pool.alloc(&[0; 60]).is_some());
    }

    #[test]
    fn big_packets_use_the_large_shelf() {
        let pool = PacketPool::new(1, 1);
        let big = pool.alloc(&vec![7; 9000]).unwrap();
        assert_eq!(big.len(), 9000);
        assert_eq!(pool.available(), (1, 0));
        assert!(pool.alloc(&vec![7; 9000]).is_none());
        assert!(pool.alloc(&vec![0; LARGE_SLOT + 1]).is_none());
        drop(big);
        assert_eq!(pool.available(), (1, 1));
    }

    #[test]
    fn writes_through_deref_mut_stay_within_the_packet() {
        let pool = PacketPool::new(1, 0);
        let mut p = pool.alloc(&[0; 4]).unwrap();
        p[3] = 5;
        assert_eq!(&*p, &[0, 0, 0, 5]);
    }
}
