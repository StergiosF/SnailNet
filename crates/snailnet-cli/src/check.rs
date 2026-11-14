//! `snailnet check`: can WinDivert load here, and does PID mapping work?
//! Answers the Phase 0 HVCI question without touching any traffic (the FLOW
//! layer is sniff-only). Each new connection is shown with the app family
//! it resolves to (PLAN.md §7).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use snailnet_core::classify::How;
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

    let attribution = snailnet_service::shaper::new_attribution();
    let mut families: HashMap<String, usize> = HashMap::new();
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
            let app = attribution.app(attribution.owner_of(f.process_id).app);
            seen += 1;
            named += (app.how != How::Unknown) as usize;
            *families.entry(app.name.clone()).or_default() += 1;
            if seen <= max_events {
                let proto = match f.protocol {
                    6 => "tcp",
                    17 => "udp",
                    _ => "ip ",
                };
                println!(
                    "  {proto} {:>21} → {:<40} pid {:>6} {} → {} ({:?})",
                    format!("{}:{}", f.local_ip(), f.local_port),
                    format!("{}:{}", f.remote_ip(), f.remote_port),
                    f.process_id,
                    if app.exe.is_empty() { "-" } else { &app.exe },
                    app.name,
                    app.how,
                );
            }
        }
    }
    println!("\n{seen} new flows, {named} attributed to an app.");
    let mut by_count: Vec<_> = families.into_iter().collect();
    by_count.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    for (name, n) in by_count {
        println!("  {n:>5}  {name}");
    }
    if seen == 0 {
        println!("No flows seen: generate some traffic and run again.");
    }
    Ok(())
}
