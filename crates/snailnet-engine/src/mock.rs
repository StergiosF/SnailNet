//! An in-memory [`PacketEngine`] for tests (PLAN.md §6.1).
//!
//! Tests [`inject`](MockEngine::inject) packets as if the driver had
//! diverted them and read back what was reinjected, with timestamps. Like
//! WinDivert after `WINDIVERT_SHUTDOWN_RECV`, a shut-down mock refuses new
//! packets (they would bypass the engine) but still hands over the ones it
//! already queued, then reports [`EngineError::Shutdown`].

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};

use crate::{Address, EngineError, PacketEngine, RecvBatch, SendBatch};

/// A packet the engine reinjected.
#[derive(Debug, Clone)]
pub struct Sent {
    pub at: Instant,
    pub data: Vec<u8>,
    pub outbound: bool,
}

type Diverted = (Vec<u8>, Address);

pub struct MockEngine {
    inbox_tx: Sender<Diverted>,
    inbox: Receiver<Diverted>,
    /// A packet that didn't fit the previous batch; goes first next time.
    carry: Mutex<Option<Diverted>>,
    sent: Mutex<Vec<Sent>>,
    shut: AtomicBool,
    stall_sends: AtomicBool,
    fail_sends: AtomicBool,
}

impl Default for MockEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MockEngine {
    pub fn new() -> Self {
        let (inbox_tx, inbox) = unbounded();
        Self {
            inbox_tx,
            inbox,
            carry: Mutex::new(None),
            sent: Mutex::new(Vec::new()),
            shut: AtomicBool::new(false),
            stall_sends: AtomicBool::new(false),
            fail_sends: AtomicBool::new(false),
        }
    }

    /// Divert one packet. Returns false once the engine is shut down: the
    /// real driver no longer captures it, it goes out untouched.
    pub fn inject(&self, data: &[u8], outbound: bool) -> bool {
        if self.shut.load(Relaxed) {
            return false;
        }
        let mut addr = Address::default();
        addr.set_outbound(outbound);
        self.inbox_tx.send((data.to_vec(), addr)).is_ok()
    }

    /// Everything reinjected so far, in order.
    pub fn sent(&self) -> Vec<Sent> {
        self.sent.lock().unwrap().clone()
    }

    pub fn sent_count(&self) -> usize {
        self.sent.lock().unwrap().len()
    }

    /// Make `send_batch` block until this is turned off again: simulates a
    /// wedged scheduler thread.
    pub fn stall_sends(&self, on: bool) {
        self.stall_sends.store(on, Relaxed);
    }

    /// Make `send_batch` fail (and drop the batch) until turned off:
    /// simulates Windows refusing our reinjected packets.
    pub fn fail_sends(&self, on: bool) {
        self.fail_sends.store(on, Relaxed);
    }

    pub fn is_shut_down(&self) -> bool {
        self.shut.load(Relaxed)
    }

    /// Wait until `n` packets were reinjected, or `timeout` passes.
    pub fn wait_for_sent(&self, n: usize, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.sent_count() >= n {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.sent_count() >= n
    }
}

impl PacketEngine for MockEngine {
    fn recv_batch(&self, batch: &mut RecvBatch) -> Result<usize, EngineError> {
        let mut carry = self.carry.lock().unwrap();
        let first = match carry.take() {
            Some(p) => p,
            None => loop {
                match self.inbox.recv_timeout(Duration::from_millis(5)) {
                    Ok(p) => break p,
                    Err(RecvTimeoutError::Timeout) if self.shut.load(Relaxed) => {
                        return Err(EngineError::Shutdown);
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return Err(EngineError::Shutdown),
                }
            },
        };
        let (buf, addrs) = batch.raw_mut();
        let (mut bytes, mut count) = (0, 0);
        let mut next = Some(first);
        while let Some((data, addr)) = next.take() {
            buf[bytes..bytes + data.len()].copy_from_slice(&data);
            addrs[count] = addr;
            bytes += data.len();
            count += 1;
            if count == addrs.len() {
                break;
            }
            match self.inbox.try_recv() {
                Ok(p) if bytes + p.0.len() <= buf.len() => next = Some(p),
                Ok(p) => *carry = Some(p),
                Err(_) => {}
            }
        }
        Ok(batch.fill(bytes, count))
    }

    fn send_batch(&self, batch: &mut SendBatch) -> Result<(), EngineError> {
        while self.stall_sends.load(Relaxed) {
            std::thread::sleep(Duration::from_millis(2));
        }
        if self.fail_sends.load(Relaxed) {
            return Err(std::io::Error::from_raw_os_error(87).into());
        }
        let at = Instant::now();
        let (buf, addrs) = batch.raw();
        let mut sent = self.sent.lock().unwrap();
        let mut off = 0;
        for a in addrs {
            let len = snailnet_core::parse::ip::packet_len(&buf[off..])
                .expect("only whole IP packets are queued");
            sent.push(Sent {
                at,
                data: buf[off..off + len].to_vec(),
                outbound: a.outbound(),
            });
            off += len;
        }
        batch.clear();
        Ok(())
    }

    fn shutdown(&self) {
        self.shut.store(true, Relaxed);
    }
}
