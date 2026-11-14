# Phase 0 report: can WinDivert + our scheduler do it?

> Status: **spike built, real-machine runs pending.** Fill in the tables
> below, then make the Go/No-go call on D4 (PLAN.md §21, "Immediate next steps").

## What was built

| Piece | Where | State |
|---|---|---|
| Workspace skeleton (§5) | `Cargo.toml`, `crates/*` | done |
| Clock trait, real + simulated | `snailnet-core/src/clock.rs` | done |
| Root token bucket + weighted DRR, N classes | `snailnet-core/src/scheduler/` | done, 15 tests |
| IPv4/IPv6/TCP/UDP header parser | `snailnet-core/src/parse/ip.rs` | done, 8 tests |
| `PacketEngine` trait + thin WinDivert FFI | `snailnet-engine` | done, DLL loads without admin |
| PID by port tables, exe names, hi-res timer | `snailnet-win` | done, 4 tests |
| `snailnet check` (driver load + PID mapping, sniff-only) | `snailnet-cli/src/check.rs` | done |
| `snailnet spike` (shaping) | `snailnet-cli/src/spike.rs` | done, **not yet run as admin** |

Simulated results (`cargo test -p snailnet-core`):

- 40/60 with both saturated: within ±3% over 2 s ✅
- Idle class's share redistributed 40:15 to the others ✅
- Byte-fair with 1500 B vs 100 B packets ✅
- 99:1 weights: the small class is never starved ✅
- Total output matches the root rate within 1% ✅

### Phase 0 decisions and simplifications (Phase 1 replaces them)

- **Thin FFI instead of the `windivert` crate** (allowed by §16): six
  functions, loaded at runtime with `libloading`, so a missing or blocked
  driver produces a readable error, not a loader crash.
- **No priority, ceilings, CoDel or express lane yet.** Queues tail-drop at
  ~50 ms of the class's share. Pure TCP control packets (SYN/ACK/FIN/RST) and
  DNS bypass the queues, a minimal express lane.
- **Attribution:** FLOW-layer 5-tuple → PID for new flows, plus a 1 s
  IP Helper snapshot by local port for flows that were already open. No
  SOCKET layer and no 50 ms pending window; unknown packets go to
  "everything else" and are counted.
- **Per-packet `Vec` allocation** on the fast path (Phase 1: slab pool).
- `--down` is the line speed, and shaping runs at `--factor` (default 0.92) of it.
- "Everything else" keeps its 5% floor even in the plan's 40/60 example, so
  the effective split is 38.1 / 57.1 / 4.8. The spike prints this at start.

## How to run (elevated terminal)

```powershell
cargo build --release
# 1. Does the driver load? (sniff-only, never touches traffic)
.\target\release\snailnet.exe check
# 2. Measure your line first (e.g. a browser speed test), then:
.\target\release\snailnet.exe spike --down <your Mbps> `
    --app steam.exe,steamwebhelper.exe=40 --app brave.exe=60
# Ctrl+C stops it; networking returns to normal immediately.
```

The `who:` line shows how packets were attributed. If **flow** stays near 0%
while traffic runs, the FLOW-layer address decoding is wrong, so report it.

Check Memory integrity (HVCI) state:

```powershell
Get-CimInstance -ClassName Win32_DeviceGuard -Namespace root\Microsoft\Windows\DeviceGuard |
  Select-Object -ExpandProperty SecurityServicesRunning   # 2 = HVCI running
```

## Results (fill in)

### Machine

| Item | Value |
|---|---|
| Windows edition / build | Windows 11 Pro, build 26300 |
| CPU | AMD Ryzen 5 9600X (6 cores) |
| Line (ISP speed test ↓/↑, idle ping) | |
| Memory integrity (HVCI) | **on** (`SecurityServicesRunning` = 2) |
| Security software | Windows Defender |

### Compatibility

| Check | Result | Notes |
|---|---|---|
| `snailnet check` loads the driver | ✅ 2026-11-14 | driver reports version 2.2 |
| …with HVCI on | ✅ | loaded with Memory integrity running |
| Security software silent? | ✅ | Defender raised nothing |
| PID mapping (FLOW layer) | ✅ | 28/28 new flows attributed to the right exe (svchost DNS, brave, node, powershell) |
| FLOW address decoding (IPv4 + IPv6) | ✅ | `words_to_ip` confirmed: e.g. `192.168.1.26`, `140.82.121.4` (GitHub), `2606:4700::…` (Cloudflare) |
| Anti-cheat: game 1 (name, AC) | | does it refuse to launch / kick? |
| Anti-cheat: game 2 | | |
| Coexists with VPN client | | |

### Shaping

| Scenario | Expected | Measured | Pass (±5%) |
|---|---|---|---|
| Steam download + YouTube 4K, 40/60 | ~38/57 when both saturate | | |
| Close YouTube | Steam back to ~full shaped rate within 2 s | | |
| Only one app busy | gets ~100% of shaped rate | | |
| `who:` attribution | flow + port ≥ 95% | | |

### Ping under load

Run `ping -t 1.1.1.1` while a big download runs.

| Condition | Ping p50 / max |
|---|---|
| Idle, no SnailNet | |
| Download, no SnailNet | |
| Download, spike factor 0.92 | |
| Download, spike factor 0.85 | |

### CPU

| Rate | `snailnet.exe` CPU % (Task Manager) |
|---|---|
| ~100 Mbps | |
| max line rate | |

## Go / No-go

- [x] Driver loads on this machine with HVCI on
- [ ] Games you play tolerate it (or Game-safe mode is enough)
- [ ] Split within ±5% while both apps saturate
- [ ] Ping under load clearly better than without SnailNet

**Decision:** _pending_. If Go, start Phase 1. If No-go, record why and
re-open D4 (`WfpEngine`).
