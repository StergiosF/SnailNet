//! FLOW and SOCKET layer events → `snailnet_core::classify` (PLAN.md §4:
//! the `flow-sniffer` and `socket-guard` threads, sniff-only for now; SOCKET
//! blocking arrives with Block per app in Phase 5).
//!
//! Three threads keep the shared [`WinAttribution`] current:
//! - `flow-sniffer`: FLOW established/deleted (exact 5-tuple → PID),
//! - `socket-sniffer`: SOCKET connect/accept/bind/listen/close, which
//!   usually arrive *before* the flow's first packet,
//! - `port-table`: the IP Helper snapshot once a second, for connections
//!   that were already open when SnailNet started.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use snailnet_core::classify::{Attribution, Event, FlowKey, Pack, Resolver};
use snailnet_engine::batch::DEFAULT_BATCH;
use snailnet_engine::windivert::{Layer, WinDivert, WinDivertLib, event, flags};
use snailnet_engine::{Address, RecvBatch};
use snailnet_win::process::{WinProcesses, windows_dir};

pub type WinAttribution = Attribution<WinProcesses>;

pub fn new_attribution() -> WinAttribution {
    let resolver = Resolver::new(Pack::builtin(), &windows_dir());
    Attribution::new(resolver, WinProcesses::new())
}

/// A FLOW or SOCKET address → classify event. `None` for anything else.
pub fn decode(a: &Address) -> Option<Event> {
    let f = a.flow();
    let key = FlowKey::new(
        f.protocol,
        (f.local_ip(), f.local_port),
        (f.remote_ip(), f.remote_port),
    );
    let (pid, endpoint) = (f.process_id, f.endpoint_id);
    Some(match a.event() {
        event::FLOW_ESTABLISHED => Event::FlowEstablished { key, pid },
        event::FLOW_DELETED => Event::FlowDeleted { key },
        event::SOCKET_CONNECT | event::SOCKET_ACCEPT => Event::Connected { key, pid, endpoint },
        event::SOCKET_BIND | event::SOCKET_LISTEN => Event::Bound { key, pid, endpoint },
        event::SOCKET_CLOSE => Event::Closed { key, endpoint },
        _ => return None,
    })
}

const FILTER: &str = "!loopback and (tcp or udp)";
const PORT_TABLE_EVERY: Duration = Duration::from_secs(1);

/// The running event threads. Open them *before* the NETWORK handle so no
/// new flow is missed.
pub struct Sniffers {
    flow: Arc<WinDivert>,
    socket: Arc<WinDivert>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl Sniffers {
    pub fn start(lib: &Arc<WinDivertLib>, attribution: Arc<WinAttribution>) -> Result<Self> {
        let sniff = flags::SNIFF | flags::RECV_ONLY;
        let flow = Arc::new(
            WinDivert::open(lib, FILTER, Layer::Flow, 0, sniff)
                .context("opening the FLOW layer (PID mapping)")?,
        );
        let socket = Arc::new(
            WinDivert::open(lib, FILTER, Layer::Socket, 0, sniff)
                .context("opening the SOCKET layer (early PID mapping)")?,
        );
        // The snapshot covers what was open before the handles above.
        attribution.set_ports(&snailnet_win::ports::snapshot());
        let stop = Arc::new(AtomicBool::new(false));
        let mut threads = Vec::new();
        for (name, handle) in [("flow-sniffer", &flow), ("socket-sniffer", &socket)] {
            let (handle, attribution) = (handle.clone(), attribution.clone());
            threads.push(
                std::thread::Builder::new()
                    .name(name.into())
                    .spawn(move || event_loop(&handle, &attribution))?,
            );
        }
        let stop2 = stop.clone();
        threads.push(
            std::thread::Builder::new()
                .name("port-table".into())
                .spawn(move || {
                    let mut next = Instant::now() + PORT_TABLE_EVERY;
                    while !stop2.load(Relaxed) {
                        std::thread::sleep(Duration::from_millis(100));
                        if Instant::now() >= next {
                            attribution.set_ports(&snailnet_win::ports::snapshot());
                            next += PORT_TABLE_EVERY;
                        }
                    }
                })?,
        );
        Ok(Self {
            flow,
            socket,
            stop,
            threads,
        })
    }

    /// Ask every thread to stop. Safe from a Ctrl+C handler.
    pub fn stopper(&self) -> impl Fn() + Send + 'static {
        let (flow, socket, stop) = (self.flow.clone(), self.socket.clone(), self.stop.clone());
        move || {
            stop.store(true, Relaxed);
            flow.shutdown_recv();
            socket.shutdown_recv();
        }
    }

    pub fn stop(self) {
        (self.stopper())();
        for t in self.threads {
            let _ = t.join();
        }
    }
}

fn event_loop(handle: &WinDivert, attribution: &WinAttribution) {
    let mut batch = RecvBatch::new(DEFAULT_BATCH);
    while handle.recv(&mut batch).is_ok() {
        for a in batch.addresses() {
            if let Some(ev) = decode(a) {
                attribution.apply(ev);
            }
        }
    }
}
