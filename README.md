# caffeinate

Keep your computer awake from the command line.

## Install

Run once with mise, or add it to your global mise config.

```sh
mise x github:Rajaniraiyn/caffinate -- caffeinate -i
mise use -g github:Rajaniraiyn/caffinate
```

For a manual install, choose an x64 or Arm64 file from [GitHub Releases](https://github.com/Rajaniraiyn/caffinate/releases).

- Windows. Open the MSI, then open a new terminal to pick up the PATH entry.
  The ZIP contains the standalone executable.
- Linux. Install the DEB with `sudo apt install ./caffeinate_*.deb` or the
  RPM with `sudo dnf install ./caffeinate-*.rpm`. Use the static musl archive
  on Alpine or older distributions. The glibc archive requires glibc 2.35
  and libdbus.
- macOS. Extract the archive and run `./caffeinate` to use this build
  explicitly. macOS also ships its own `caffeinate`.

To build from source, use Rust 1.91 or newer. Windows needs the MSVC build tools. Linux needs `pkg-config` and libdbus headers on glibc, or a C compiler on musl.

```sh
cargo install --locked --git https://github.com/Rajaniraiyn/caffinate
```

## Use

```sh
caffeinate -i                 # allow the display to turn off
caffeinate -d -t 60           # keep the display on for a minute
caffeinate -i make            # keep awake while a command runs
caffeinate -i -w 1234         # keep awake until a process exits
```

Press Ctrl+C to end a request. Run `caffeinate --help` for all options.

On Windows and macOS, `-s` applies on AC power. On Linux, sleep inhibition
needs logind or elogind, and display inhibition needs a compatible desktop
session service. Linux has no disk-idle request, and `-w` needs Linux 5.3
or newer.

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
