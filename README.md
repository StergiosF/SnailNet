# SnailNet

Put the bandwidth hogs in the slow lane.

SnailNet shares this PC's internet between apps and websites by
percentage. Give the game download 40% and YouTube 60%, and both keep
going. A share is a guarantee when the line is busy, never a cage: what
one app doesn't use goes to whoever needs it. Calls and games can go
first, so ping stays low while something big downloads.

Windows 10 and 11, x64.

## How it works

- A Windows service (`SnailNet`, runs as LocalSystem) does the work. It
  sees this PC's packets through the WinDivert driver, tells which app
  and which website each connection belongs to, and sends them out in
  the shares you set, slightly below your line speed so the queue stays
  on your PC and not in your router.
- The app (tray icon, flyout, main window) only talks to the service. You
  can close it; your rules keep working.
- Modes switch whole sets of rules at once: Balanced, Streaming, Gaming,
  Quiet downloads, and your own copies. Ctrl+Alt+S steps through them
  (other keys in Settings → Windows).

## Help, my internet broke

SnailNet is built to step aside rather than break your connection: if the
service crashes or stops, packets flow straight through again, and after
three failures in ten minutes it goes into Safe mode and leaves your
traffic alone until it's restarted. If something still seems wrong, try
these in order. Each one is enough on its own.

1. **Pause it.** Click the snail in the tray, then Pause → "Until I
   resume" (the main window has Pause too). Nothing is shaped
   or blocked while paused. If the internet comes back, a rule is the
   problem: check for a blocked app ("Using your internet" lists blocked
   apps with Unblock) or a very small slice.
2. **Stop the service.** Open *Services* (Win+R, `services.msc`), find
   **SnailNet**, click Stop. Or, in a terminal run as administrator:

   ```
   net stop SnailNet
   ```

   To keep it from starting again at the next boot:
   `sc config SnailNet start= disabled` (`start= auto` undoes it).
3. **Uninstall it.** Settings → Apps → Installed apps → SnailNet →
   Uninstall. This stops and removes the service and the driver files.
   It asks whether to keep your settings in `C:\ProgramData\SnailNet`.
4. **Still nothing?** Restart the PC. The WinDivert driver is loaded only
   while SnailNet uses it, so after a restart with the service stopped or
   uninstalled, nothing of SnailNet is in the network path. If the
   internet is still down then, the cause is elsewhere.

When you report a problem, the service's logs help: they're in
`C:\ProgramData\SnailNet\logs` (one file a day, the last seven kept).

## Privacy

- **Nothing about you leaves your PC.** No telemetry, no accounts, no
  analytics.
- **What SnailNet reads, on your PC only:** which program owns each
  connection (its name, path, publisher), the addresses it talks to, and
  the *name* of the website when the connection starts (the server name
  in the TLS or QUIC hello, and the answers to DNS lookups). It never
  reads what you send or receive.
- **What it keeps:** your rules, modes, settings and measured speeds
  (`C:\ProgramData\SnailNet\config.toml`), the list of apps it has seen
  so it knows which are new (`known-apps.txt` there), and seven days of
  logs, which name apps and networks. Your hotkey and window choices are
  in your own app data folder. Uninstalling can delete all of it.
- **The only connections SnailNet makes itself:**
  - the update check, to GitHub (`github.com/StergiosF/SnailNet`
    releases), only when you click Check for updates in Settings;
  - the website and game lists, from the same GitHub releases, once a
    day (two small downloads, checked against SnailNet's signing key
    before use; kept in `C:\ProgramData\SnailNet\packs.toml`). Turn it
    off in Settings → About → "Keep website and game lists up to date";
  - the speed test, to Cloudflare (`speed.cloudflare.com`), only when
    you click Measure;
  - pings to your router and to `1.1.1.1` while SnailNet is shaping
    (once a second while the line is busy, every 30 seconds when it's
    quiet), to keep the shaping just below what your line can take.

## Build

Rust (stable, MSVC) and Node (current LTS).

```powershell
cargo test --workspace
# Dev loop: the service elevated, then the app.
target\debug\snailnet-service.exe run --console
cd app; npm ci; npm run tauri dev
# The NSIS installer, signed for updates:
.\scripts\build-installer.ps1
```

`PLAN.md` is the design and the record of every decision;
`docs/HANDOFF.md` says where the work stands.

## Licences

- **SnailNet** is free software under the GNU General Public License,
  version 3 or (at your option) any later version: see `LICENSE`
  (shipped as `licenses\SnailNet-LICENSE.txt` in the install folder).
  Copyright © 2026 Stergios Fotoglou.
- **WinDivert** 2.2.2 (driver and DLL, unmodified, loaded at run time) is
  used under the GNU LGPL v3 (it is dual-licensed LGPLv3 / GPLv2). Its
  licence ships as `licenses\WinDivert-LICENSE.txt` in the install folder,
  and its source is at
  <https://github.com/basil00/WinDivert/releases/tag/v2.2.2>.
- Everything else SnailNet ships (Rust crates, Tauri, the Atkinson
  Hyperlegible Next font) is under permissive licences (MIT, Apache-2.0,
  BSD, ISC, Zlib, Unicode, OFL) or MPL-2.0 (used unmodified).
  `licenses\THIRD-PARTY-NOTICES.txt` in the install folder lists each one
  with its licence text; `scripts\third-party-notices.ps1` makes it at
  build time, and `deny.toml` keeps the list of allowed licences.
