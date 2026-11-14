# WinDivert 2.2.2 (pinned)

Upstream: <https://github.com/basil00/WinDivert/releases/tag/v2.2.2>
(`WinDivert-2.2.2-A.zip`, published 2022-09-21; still the latest release).
Licence: dual LGPLv3 / GPLv2, see `LICENSE`. SnailNet loads the DLL
dynamically and ships it unmodified, which satisfies the LGPL (PLAN.md §16).

| File | SHA-256 |
|---|---|
| `WinDivert-2.2.2-A.zip` (download) | `63cb41763bb4b20f600b6de04e991a9c2be73279e317d4d82f237b150c5f3f15` |
| `x64/WinDivert64.sys` | `8da085332782708d8767bcace5327a6ec7283c17cfb85e40b03cd2323a90ddc2` |
| `x64/WinDivert.dll` | `c1e060ee19444a259b2162f8af0f3fe8c4428a1c6f694dce20de194ac8d7d9a2` |

## Signature

- `WinDivert64.sys`: Authenticode **Valid**, signer a Chengdu company
  (成都密思听科技有限公司), the certificate the upstream author used for 2.2.2.
  The hash matches the driver discussed in upstream issue #401.
- `WinDivert.dll`: not signed (upstream ships it unsigned).
- The signing certificate has **expired**, but the signature is timestamped,
  so Windows still accepts it. Some security products block it anyway
  (upstream issues #401, #266). Record what yours does in
  `docs/phase0-report.md`.

`include/windivert.h` is kept for reference: the FFI in
`crates/snailnet-engine/src/windivert.rs` and `address.rs` mirrors it.
