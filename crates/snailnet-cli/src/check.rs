//! `snailnet check`: can WinDivert load here, and does PID mapping work?
//! Answers the Phase 0 HVCI question without touching any traffic (the FLOW
//! layer is sniff-only).

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use snailnet_engine::RecvBatch;
use snailnet_engine::windivert::{Layer, WinDivert, WinDivertLib, event, flags};

pub fn run(seconds: u64, max_events: usize) -> Result<()> {
    let lib = WinDivertLib::load()?;
    println!("WinDivert.dll   {}", lib.path().display());
    let flow = Arc::new(
        WinDivert::open(
            &lib,
            "!loopback",
            Layer::Flow,
            0,
            flags::SNIFF | flags::RECV_ONLY,
        )
        .context("opening a sniff-only FLOW handle")?,
    );
    match flow.driver_version() {
        Some((maj, min)) => println!("driver          loaded, version {maj}.{min}"),
        None => println!("driver          loaded (version unknown)"),
    }
    println!("watching new connections for {seconds}s (open a web page)…\n");

    let stopper = flow.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(seconds));
        stopper.shutdown_recv();
    });

    let mut batch = RecvBatch::new(16);
    let (mut seen, mut named) = (0usize, 0usize);
    loop {
        match flow.recv(&mut batch) {
            Ok(_) => {}
            Err(snailnet_engine::EngineError::Shutdown) => break,
            Err(e) => return Err(e.into()),
        }
        for a in batch.addresses() {
            if a.event() != event::FLOW_ESTABLISHED {
                continue;
            }
            let f = a.flow();
            let exe = snailnet_win::process::exe_name(f.process_id);
            seen += 1;
            named += exe.is_some() as usize;
            if seen <= max_events {
                let proto = match f.protocol {
                    6 => "tcp",
                    17 => "udp",
                    _ => "ip ",
                };
                println!(
                    "  {proto} {:>21} → {:<40} pid {:>6} {}",
                    format!("{}:{}", f.local_ip(), f.local_port),
                    format!("{}:{}", f.remote_ip(), f.remote_port),
                    f.process_id,
                    exe.as_deref().unwrap_or("?"),
                );
            }
        }
    }
    println!("\n{seen} new flows, {named} attributed to an exe.");
    if seen == 0 {
        println!("No flows seen: generate some traffic and run again.");
    }
    Ok(())
}
