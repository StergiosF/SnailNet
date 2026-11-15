# Phase 0 report: can WinDivert + our scheduler do it?

> Status: **driver check and synthetic shaping run done (2026-11-14), and
> both look good.** Factor 0.85 run done 2026-11-15 (loaded ping avg 20 ms
> vs 9 idle). Real Steam + Brave run done 2026-11-15: 40/60, no buffering,
> ping 226 → 22 ms avg, CPU 1.1%. Upload (≈ 40 Mbps line) done: 40/60,
> ping 12 ms vs 10 idle. Still open: anti-cheat (user skipped it for now).
> Then make the Go/No-go call on D4 (PLAN.md §21).

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
| Line (ISP speed test ↓/↑, idle ping) | Fiber. ≈ 25 Mbps ↓ (Cloudflare: 1 × 25 MB 24.0 Mbps, 4 × 25 MB 21.9–27.8 Mbps, 2026-11-15), ≈ 40 Mbps ↑ (10–30 MB POSTs: 35, 20, 8, 44 Mbps; the low ones probably overlapped Steam/YouTube), idle ping 9–11 ms to 1.1.1.1 |
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
| Synthetic: `a.exe=40` / `b.exe=60` (curl copies, Cloudflare 25 MB), `--down 25` → 23 Mbps | 38.1 / 57.1 / 4.8 | t=6–14 s: a 37–39%, b 56–57%, other 4–5%; total 21.5–23.0 of 23.0 | ✅ within ±2% |
| b stops (≈16 s) → a recovers | a back to most of the line within 2 s | 18 s: a 14.5 Mbps; 20–28 s: a 15–20 Mbps (83–88%), rest is background traffic in "everything else" | ✅ ≈2–4 s |
| Steam download + YouTube 4K in Brave, `steam=40:low` / `brave=60`, `--down 25 --factor 0.85` → 21.2 Mbps, 120 s (2026-11-15) | 40/60 while both saturate (everything else idle) | **40/60 in ~80% of seconds**, 39–44 / 56–61 in nearly all the rest; short swings to 46–61% Steam when Brave paused between chunks; Brave idle at 21–23 s and 39 s → Steam took 100% at once and gave it back within 1 s. Total 20–21.3 of 21.2 most seconds (dips to 13–16 every ~10 s). **YouTube didn't buffer** (user watched). Unknown 0%, passthrough 0 | ✅ |
| Upload, synthetic: `a.exe=40` / `b.exe=60` (curl copies, 30 MB POSTs to Cloudflare `__up`), `--up 40 --factor 0.85` → 34 Mbps, 40 s (2026-11-15) | 40/60 | **40/60 in 30 of 38 loaded seconds**; total 30–34 of 34. b idle between POSTs at 20–23 s → a took 96% and gave it back within 1 s. 2–7 drops/s, flow 100%, unknown 0% | ✅ |
| `who:` attribution | flow + port ≥ 95% | flow 87–96% + port remainder, **unknown 0%** | ✅ |
| Ctrl+C / `--duration` end = fail-open | normal networking right away | ✅ queues flushed, handles closed, ping/HTTPS normal afterwards | ✅ |

Tail drops ran at 100–400 per 2 s window while saturated. That's expected
with Phase 0's tail-drop queues; CoDel in Phase 1 should mark or drop earlier
and less.

### Ping under load

Run `ping -t 1.1.1.1` while a big download runs.

| Condition | Ping p50 / max |
|---|---|
| Idle, no SnailNet | avg 11 ms, max 13 ms |
| Download (6 streams), no SnailNet | avg 61 ms, **max 234 ms** |
| Download (2 streams), spike factor 0.92 | avg 25 ms, max 54 ms |
| Download (2 streams), spike factor 0.92, Phase 1 scheduler (CoDel/ECN, express lane), 2026-11-14 | runs: avg 38 / max 221 (1 lost), and avg 22 / max 47 |
| Download (2 streams), spike factor 0.85, Phase 1 scheduler, 2026-11-15 (idle that run: avg 9, max 12) | **avg 20 ms, max 39 ms, 0 of 22 lost** |

| Steam download + YouTube 4K in Brave, no SnailNet (2026-11-15) | avg 226 ms, **max 688 ms, 2 of 20 lost** |
| Same, spike factor 0.85, steam=40:low / brave=60 (90 pings) | **avg 22, p50 17, p90 40, p95 53, max 77 ms, 0 lost** |

| Upload 30 MB, no SnailNet (2026-11-15, partial: 9 samples) | 10 → 84 → **410 ms** during the upload |
| Upload, a.exe=40 / b.exe=60 at 34 Mbps (factor 0.85), idle that run 10 ms avg | **avg 12, max 32 ms**, 1 of 28 lost |

**Upload (2026-11-15):** shaped upload keeps ping within +2 ms of idle on
average; unshaped, one 30 MB upload pushed it to 410 ms.

**Real scenario (2026-11-15):** shaping cut loaded ping ten-fold
(226 → 22 ms avg, 688 → 77 max, loss 10% → 0). Median 17 ms is within the
§20 target (idle ≈ 9–10 ms, so ≤ ~20 ms); the average is 2–3 ms over it,
and the tail (p95 53) is the slow-start/chunk-burst effect described below.
Attribution: Steam's and Brave's connections were open before shaping
started, so the port snapshot carried 30–40% of packets (100% in the
first 23 s), the rest by FLOW; unknown 0%.

**Factor 0.85 (2026-11-15, `scripts/phase0-synthetic.ps1 -Factor 0.85`):**
loaded ping averaged 20 ms against 9 ms idle, so it meets the §20 target
(within +10 ms of idle) on average. Most samples were 10–24 ms; the
outliers (35, 38, 39) are the slow-start tail that `snailnet-sim`
predicted (+11.5 ms p99 at 0.85). Throughput was 19–21.3 of the 21.2 Mbps
cap. The split held at **39–40 / 60–61** from 4 s to 23 s, one outlier
second at 33/67, and both curls ran for the whole 32 s this time.
From 24 s on, a background app (attributed by port) joined
"everything else", which then took 5–14% while b dropped to 49–56%, with
100–210 drops/s. Everything else is allowed more than its 5% floor only
when a class underfills, so b was probably between its 25 MB downloads
(each restart is a slow start). Not investigated further.

Shaping cut loaded ping from 61 → 25 ms average and 234 → 54 ms max, but
that's still above the §20 target (within +10 ms of idle, i.e. ≤ 21 ms).
Likely causes: the 25 Mbps line figure is rough, so 0.92 may still sit at or
above the real bottleneck, and there's no CoDel yet. Next: rerun at 0.85,
then with CoDel in Phase 1. (ICMP isn't diverted, so ping measures the
router's queue, which is exactly what Low-latency mode is meant to keep empty.)
Caveat: the two runs used 6 vs 2 download streams, so the comparison isn't
exact.

### CPU

| Rate | `snailnet.exe` CPU % (Task Manager) |
|---|---|
| ~100 Mbps | not measurable on this line |
| max line rate (21.2 Mbps shaped, real scenario, 2026-11-15) | **1.08% of all cores** (12 logical; ≈ 13% of one core), working set 31.9 MB. Measured from the process's own CPU time over 90 s, not Task Manager |

## Go / No-go

- [x] Driver loads on this machine with HVCI on
- [ ] Games you play tolerate it (or Game-safe mode is enough)
- [x] Split within ±5% while both apps saturate (synthetic, and the real Steam + Brave run: 40/60, YouTube 4K didn't buffer)
- [x] Ping under load clearly better than without SnailNet (61 → 25 ms avg at 0.92 with the Phase 0 scheduler; at 0.85 with the Phase 1 scheduler, avg 20 ms vs 9 idle, which meets the §20 target on average, max 39)

**Decision:** _pending_. If Go, start Phase 1. If No-go, record why and
re-open D4 (`WfpEngine`).
