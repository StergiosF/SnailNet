# Fuzzing the packet parsers

Targets for the parsers that read bytes off the wire (PLAN.md §8.1):

| Target | What it feeds |
|---|---|
| `tls` | any bytes to `parse::tls::from_tcp` (whole, and as growing 1400-byte segments) and `from_handshake`; a returned SNI must be a valid lowercase host name |
| `quic` | any datagram to `parse::quic::open_initial`; any CRYPTO frame layout through `CryptoAssembler` into `tls::from_handshake` |
| `dns` | any message to `parse::dns::answers`; returned names must be valid host names |

This crate is its own workspace and needs nightly, so the stable
`cargo test --workspace` at the repo root never builds it.

```powershell
rustup toolchain install nightly
cargo install cargo-fuzz
cd crates\snailnet-core
cargo +nightly fuzz run tls -- -max_total_time=300
cargo +nightly fuzz run quic -- -max_total_time=300
cargo +nightly fuzz run dns -- -max_total_time=300
```

On Windows, libFuzzer needs the MSVC toolchain with AddressSanitizer
(the "C++ AddressSanitizer" component of the VS Build Tools). If ASan
gives trouble, `cargo +nightly fuzz run <target> -s none` fuzzes without
it (panics are still found). Crashes land in `fuzz/artifacts/<target>/`;
replay one with `cargo +nightly fuzz run <target> <file>`.
