# caffeinate

Keep your PC awake from the command line.

## Install

Run it once with mise. Ctrl+C ends the request.

```powershell
mise x github:Rajaniraiyn/caffinate@0.1.1 -- caffeinate -disu
```

Add it to your global mise config with `mise use -g github:Rajaniraiyn/caffinate@0.1.1`.
Use `mise install github:Rajaniraiyn/caffinate@0.1.1` to download the release
without changing that config.

Cargo builds from the tagged source and needs Rust 1.91 or newer and the MSVC
build tools.

```powershell
cargo install --locked --git https://github.com/Rajaniraiyn/caffinate --tag v0.1.1
```

## Use

```powershell
caffeinate /timeout 60
caffeinate /display /wait $PID
caffeinate /idle cmd.exe /d /c 'echo Awake'
```

The default keeps the system from idling to sleep. A command takes precedence
over `/wait` and keeps its exit code. A timeout releases the request while the
command can continue running.

Run `caffeinate /?` for the full switch list. Use `--` before a command name
that looks like a switch. System power policy controls when requests take
effect. [Power request behavior](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-powersetrequest)

## Release

For a manual release, set the version in `Cargo.toml`, run
`cargo update --workspace`, and commit both Cargo files. Push the matching
`vMAJOR.MINOR.PATCH` tag to publish x64 and Arm64 ZIPs and their checksums.

To pick the version automatically, run the Release workflow from the default
branch with the tag field empty. It uses the manifest version for the first
release, then picks the next patch unless the manifest already specifies a
higher version. Version bumps are committed under the release tag. To retry
after the tag exists, run the workflow with that tag.
