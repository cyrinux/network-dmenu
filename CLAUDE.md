# Claude Code Development Notes

## Platform

network-dmenu is a **Linux-only** dmenu-style network tool. It shells out to
`nmcli`, `iwctl`, `bluetoothctl`, `tailscale`, `firewall-cmd`, `tor`, `ssh`
and friends, and talks to the desktop over D-Bus for notifications.

## Layout

```
src/
├── main.rs           # CLI (clap), ActionType enum, action dispatch, config loading
├── lib.rs            # Public API: action parsing helpers, config structs, re-exports
├── streaming.rs      # Builds the menu progressively from async action producers
├── command.rs        # CommandRunner trait (RealCommandRunner + mocks in tests)
├── networkmanager.rs # nmcli WiFi/VPN actions
├── iwd.rs            # iwd WiFi actions
├── bluetooth.rs      # bluetoothctl actions
├── tailscale.rs      # Tailscale, Mullvad exit nodes, Tailscale Lock  [feature: tailscale]
├── tailscale_prefs.rs                                                 [feature: tailscale]
├── firewalld.rs      # firewall-cmd zones and panic mode              [feature: firewalld]
├── diagnostics.rs    # ping/traceroute/speedtest/dns-bench actions
├── nextdns.rs, tor.rs, ssh.rs, rfkill.rs, dns_cache.rs, port_utils.rs, privilege.rs
├── notifications.rs, logger/, constants.rs, utils.rs
```

Features (`Cargo.toml`): `tailscale` and `firewalld`, both on by default.
Everything must build and pass clippy/tests **with and without** default features;
gate code and tests with `#[cfg(feature = "...")]`.

## Conventions

- Functional style: pure parsing functions, `Result` for errors, no panics in library code.
- Every external command goes through `CommandRunner` so it can be mocked in unit tests.
- Shared helpers live in `lib.rs`; do not duplicate them in `main.rs`.
- Test modules go at the end of each file (clippy `items_after_test_module`).
- Keep `regex` default features: the code relies on `(?i)` and `\d`.

## Commands

```bash
just check          # fmt + clippy (both feature sets, -D warnings) + tests
just lint
just test
cargo deny check
```

CI (`.github/workflows/rust.yml`) runs exactly `just check`'s steps on every push and PR.

## Releasing

Releases are GitHub releases built by `.github/workflows/release.yml` on tag push
(x86_64 and aarch64 binaries via `cross`). The crate is **not** published to crates.io anymore.

1. Bump `version` in `Cargo.toml`, run `cargo update -p network-dmenu` to refresh `Cargo.lock`.
2. Commit, then tag with the bare version (`git tag -s 3.1.0`) and push the tag.
3. Watch the Release workflow; release notes are auto-generated from merged commits.
