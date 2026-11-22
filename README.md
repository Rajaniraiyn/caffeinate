# caffeinate

Keep your computer awake from the command line.

## Install

Releases are available for Windows, macOS, and Linux on x64 and Arm64.
Run once with mise. Ctrl+C ends the request.

```powershell
mise x github:Rajaniraiyn/caffinate@0.2.0 -- caffeinate -isu
```

Add it to your global mise config with `mise use -g github:Rajaniraiyn/caffinate@0.2.0`.
Use `mise install github:Rajaniraiyn/caffinate@0.2.0` to download the release
without changing that config.

Linux binaries need glibc 2.35 or newer and libdbus. To build from source,
use Rust 1.91 or newer. Windows builds need the MSVC build tools. Linux builds
also need `pkg-config` and libdbus headers, such as `libdbus-1-dev` on
Debian or Ubuntu.

```sh
cargo install --locked --git https://github.com/Rajaniraiyn/caffinate --tag v0.2.0
```

## Use

```powershell
caffeinate /timeout 60
caffeinate /display /wait $PID
caffeinate /idle cmd.exe /d /c 'echo Awake'
```

On any supported platform, use short options or `--help`.

```sh
caffeinate -it 60
caffeinate -i make
```

The default keeps the system from idling to sleep. A command takes precedence
over `-w` and keeps its exit code. A timeout releases the request while the
command can continue running.

Use `--` before a command name that looks like a switch. System power policy
controls when requests take effect.

On macOS, IOKit supplies the idle, display, system, disk and user-activity
assertions. The wrapped command runs as a child process. On macOS and Windows,
`-s` applies only on AC power.
[Apple's reference implementation](https://github.com/apple-oss-distributions/PowerManagement/blob/main/caffeinate/caffeinate.c)

On Linux, `-i` blocks logind or elogind suspend, and `-s` also blocks its idle
action. Display options need a compatible ScreenSaver or GNOME session
service. Waking a display with `-u` depends on that service. A bare window
manager may provide neither. Linux has no portable
disk-idle request. Watching an existing PID with `-w` uses pidfds, available
from Linux 5.3.
[logind inhibitors](https://systemd.io/INHIBITOR_LOCKS/) · [Desktop idle inhibition](https://specifications.freedesktop.org/idle-inhibit/0.1/)

## Release

For a manual release, set the version in `Cargo.toml`, run
`cargo update --workspace`, and commit both Cargo files. Push the matching
`vMAJOR.MINOR.PATCH` tag to publish the archives and SHA-256 checksums.

To pick the version automatically, run the Release workflow from the default
branch with the tag field empty. It uses the manifest version for the first
release, then picks the next patch unless the manifest already specifies a
higher version. Version bumps are committed under the release tag. To retry
after the tag exists, run the workflow with that tag.
