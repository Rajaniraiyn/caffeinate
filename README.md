# caffeinate

[![Latest release](https://img.shields.io/github/v/release/Rajaniraiyn/caffeinate)](https://github.com/Rajaniraiyn/caffeinate/releases/latest)
[![Build status](https://github.com/Rajaniraiyn/caffeinate/actions/workflows/check.yml/badge.svg?branch=main)](https://github.com/Rajaniraiyn/caffeinate/actions/workflows/check.yml?query=branch%3Amain)
[![License: MIT or Apache-2.0](https://img.shields.io/badge/license-MIT%20or%20Apache--2.0-blue.svg)](#license)

Keep your computer awake from the command line. Prevent idle sleep, keep the
display on, or hold a power request while a command or process runs.

[Download caffeinate](#install) · [Run with mise](#mise)

## Use

```sh
caffeinate -i                  # Prevent idle sleep and let the display turn off
caffeinate -d -t 300           # Keep the display on for five minutes
caffeinate -i -- make          # Stay awake until make exits
caffeinate -i -w 1234          # Stay awake until process 1234 exits
```

| Option | What it does |
| --- | --- |
| `-i`, `--idle` | Prevent idle sleep. This is the default when you request no other power option. |
| `-d`, `--display` | Keep the display on. |
| `-s`, `--system` | Request system sleep inhibition. |
| `-u`, `--user-active` | Signal user activity for five seconds or the time set by `-t`. |
| `-m`, `--disk` | Prevent disk idle sleep on macOS. |
| `-t`, `--timeout SECONDS` | Release the request after this many seconds. Zero means no timeout. |
| `-w`, `--wait PID` | Release the request when a process exits. |
| `-h`, `--help` | Show usage. |

Press Ctrl+C to end a standalone request. A wrapped command takes precedence
over `-w`. If a timeout expires first, the command keeps running and
caffeinate still returns its exit status.

## Platform support

✓ Supported · △ Depends on power, session services, or kernel version · — Unavailable

| Feature | Windows | macOS | Linux |
| --- | --- | --- | --- |
| Idle sleep `-i` | △ Modern Standby battery limit | ✓ | △ Sleep service |
| Display `-d` | ✓ | ✓ | △ Desktop session service |
| System sleep `-s` | △ On AC power | △ On AC power | △ Sleep service |
| User activity `-u` | ✓ | ✓ | △ Desktop session service |
| Disk idle `-m` | — | ✓ | — |
| Timeout `-t` | ✓ | ✓ | ✓ |
| Watch a PID `-w` | ✓ | ✓ | △ Kernel 5.3+ |
| Wrap a command | ✓ | ✓ | ✓ |

On Windows devices with Modern Standby, battery power limits sleep inhibition
to five minutes past the system's sleep timeout.
[Windows power requests](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-powersetrequest)

On Linux, sleep inhibition uses logind, elogind, or ConsoleKit2.
The sleep service may require authorization for remote sessions.
Wrapped commands also work when pidfds are unavailable.
Display behavior depends on the desktop session.

| Session service | Keep display on `-d` | Signal activity `-u` |
| --- | --- | --- |
| Xfce or MATE screensaver | ✓ | ✓ |
| Freedesktop ScreenSaver, as used by KDE Plasma and compatible desktops | ✓ | △ If the service implements `SimulateUserActivity` |
| GNOME Session Manager | ✓ | △ Holds the idle inhibitor without waking the display |
| Headless or bare window manager without a session service | — | — |

## Install

| Download | x64 | Arm64 |
| --- | --- | --- |
| Windows installer | [MSI][win-x64-msi] | [MSI][win-arm64-msi] |
| Windows portable | [ZIP][win-x64-zip] | [ZIP][win-arm64-zip] |
| macOS | [Archive][mac-x64] | [Archive][mac-arm64] |
| Debian and Ubuntu | [DEB][deb-x64] | [DEB][deb-arm64] |
| Fedora and other RPM-based distributions | [RPM][rpm-x64] | [RPM][rpm-arm64] |
| Linux static, including Alpine | [musl][musl-x64] | [musl][musl-arm64] |
| Linux with glibc 2.35+ and libdbus | [glibc][glibc-x64] | [glibc][glibc-arm64] |

The MSI adds caffeinate to your user PATH. Open a new terminal after
installation. Install a downloaded Linux package with
`sudo apt install ./caffeinate-linux-*.deb` or
`sudo dnf install ./caffeinate-linux-*.rpm`.
On macOS, run `./caffeinate` from the extracted archive to select this build.

[Release notes and checksums](https://github.com/Rajaniraiyn/caffeinate/releases/latest)

### mise

```sh
mise x github:Rajaniraiyn/caffeinate -- caffeinate -i
mise use -g github:Rajaniraiyn/caffeinate
```

### Cargo

Build with stable Rust 1.91 or newer. Windows requires the MSVC build tools.
Linux requires `pkg-config` and libdbus headers for glibc, or a C compiler for musl.

```sh
cargo install --locked --git https://github.com/Rajaniraiyn/caffeinate
```

## Rust library

```toml
[dependencies]
caffeinate = { git = "https://github.com/Rajaniraiyn/caffeinate", default-features = false }
```

```rust
let awake = caffeinate::Request::default().acquire()?;
// Do work while the request is active.
drop(awake);
```

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).

[win-x64-msi]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-windows-x64.msi
[win-arm64-msi]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-windows-arm64.msi
[win-x64-zip]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-windows-x64.zip
[win-arm64-zip]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-windows-arm64.zip
[mac-x64]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-macos-x64.tar.gz
[mac-arm64]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-macos-arm64.tar.gz
[deb-x64]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-linux-x64.deb
[deb-arm64]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-linux-arm64.deb
[rpm-x64]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-linux-x64.rpm
[rpm-arm64]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-linux-arm64.rpm
[musl-x64]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-linux-x64-musl.tar.gz
[musl-arm64]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-linux-arm64-musl.tar.gz
[glibc-x64]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-linux-x64-glibc.tar.gz
[glibc-arm64]: https://github.com/Rajaniraiyn/caffeinate/releases/latest/download/caffeinate-linux-arm64-glibc.tar.gz
