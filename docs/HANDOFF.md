# Handoff: SnailNet, Phase 0 spike

Paste this into a fresh session, or say "read docs/HANDOFF.md and continue".

## Where things stand (2026-11-14)

- **Plan:** `PLAN.md` is the source of truth (decisions D1–D20, roadmap §21).
- **Branch:** `phase0-spike` (off `main`). Phase 0 code is committed there.
- **Phase 0 status:** spike built and unit-tested. `snailnet check` has run
  elevated on the real machine: the driver loads **with HVCI on**, Defender is
  silent, and PID mapping and IPv4/IPv6 address decoding are confirmed
  (28/28 flows attributed). **Still to do:** run `spike` with real traffic
  (split accuracy, ping under load) and the anti-cheat checks. Results go in
  `docs/phase0-report.md`, which also has the how-to and the tables.
- The user's browser is **Brave** (`brave.exe`), not Chrome.
- Elevated runs: this shell isn't elevated. Launch one elevated process with
  `Start-Process powershell -Verb RunAs -Wait -ArgumentList '-File script.ps1'`,
  have the script redirect output to a file in scratch, then read it. The
  user has to click Yes on the UAC prompt.

## What exists

| Crate | Role | Tests |
|---|---|---|
| `snailnet-core` | `Clock` (real + sim), root token bucket + weighted DRR scheduler, IPv4/IPv6/TCP/UDP parser. No OS deps. | 23 |
| `snailnet-engine` | `PacketEngine` trait, `WinDivertLib`/`WinDivert` (runtime-loaded FFI via `libloading`), `Address` (80-byte `WINDIVERT_ADDRESS`), batch buffers | 6 |
| `snailnet-win` | exe name by PID, PID-by-local-port snapshot (IP Helper), high-res waitable timer | 4 |
| `snailnet-cli` (bin `snailnet`) | `check` (sniff-only driver/PID test), `spike` (shaping) | 6 |

WinDivert 2.2.2 is pinned in `third_party/windivert/` (hashes and signature
notes in its README). The DLL is found via `$SNAILNET_WINDIVERT_DIR`, then the
exe's folder, then that repo path.

## Verify

```powershell
cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo test --workspace
cargo build --release
# elevated:
.\target\release\snailnet.exe check
.\target\release\snailnet.exe spike --down <Mbps> --app steam.exe,steamwebhelper.exe=40 --app brave.exe=60
```

## Known gaps and open questions

- The shaping path (`spike`: NETWORK-layer divert, reinject, pacing) has not
  run on a live system yet. Only the sniff-only `check` path has.
- Phase 0 simplifications (Phase 1 replaces them): no priority, ceilings,
  CoDel/ECN or real express lane (only DNS and pure TCP control packets
  bypass); tail drop at ~50 ms of the class's share; a `Vec` allocated per
  packet; no SOCKET layer and no pending window.
- "Everything else" keeps a 5% floor, so 40/60 rules shape as 38.1/57.1/4.8.
- The driver's signing cert (a Chengdu company, the one upstream used for
  2.2.2) has expired; its timestamp keeps it valid, but some AVs block it.

## Next steps

1. Run `spike` as Administrator with real traffic, plus the anti-cheat
   checks; fill in `docs/phase0-report.md` (`check` is done).
2. Go/No-go on D4 (WinDivert) from that report.
3. If Go → Phase 1 (PLAN.md §21): full scheduler (priority, ceilings,
   FQ inside Everything else, CoDel/ECN, express lane), `snailnet-sim`, flow
   table + app families, Windows service, config, IPC, `snailnet-cli top`.

## Working agreements in this environment

- Commit only when asked; work on a branch, never directly on `main`.
- No browser automation unless explicitly asked (`~/.claude/rules`).
- Library or API docs: look them up with Context7 first.
- Ask interview-style questions through the options window
  (AskUserQuestion), recommended option first.
- A **GateGuard** hook blocks the first Write of every new file and the first
  Bash/PowerShell call until you state: who calls the file, which data it
  touches, and the user's verbatim instruction. State that, then retry the
  identical call.
