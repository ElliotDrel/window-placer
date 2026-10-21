# Window Placer

Give Windows apps custom default sizes and positions, once when their windows open.
Native Rust, Windows events, no polling, no service and no AutoHotkey requirement.

## Download and use

1. Download `window-placer-windows-x64.zip` from [Releases](https://github.com/ElliotDrel/window-placer/releases/latest) and extract it.
2. Edit `windows.conf` to choose your apps and rectangles.
3. Preview from PowerShell without moving windows:

```powershell
Start-Process .\window-placer.exe -ArgumentList '--duration','10' -Wait -RedirectStandardOutput preview.txt -RedirectStandardError errors.txt
Get-Content preview.txt
```

4. Close other window-placement automation, then double-click `Setup.cmd`. It installs for your user, creates the login shortcut, launches that exact shortcut and checks that the app stays running. No administrator account is needed. PowerShell equivalent:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1 -StartNow
```

Install for the current user under `%USERPROFILE%\Applications\WindowPlacer`, with a login
shortcut. Existing configuration is preserved, including migration from the earlier AppData location. Edit that folder's
`windows.conf`, then double-click the installed `Setup.cmd` to restart with your changes.
Double-click `Status.cmd` to check the process, saved shortcut and recent diagnostics.
Keep the installed folder; the extracted download can be discarded.

Setup starts the app independently of the terminal or automation tool that launched it.
The shortcut runs at sign-in after a restart, provided Windows Settings > Apps > Startup
has not disabled it. Setup preserves that Windows preference. It is a login app, not a
Windows service or crash-restarting supervisor.

Advanced installation: add `-InstallDirectory "C:\Tools\WindowPlacer"` to the command
above. `-StartupDirectory` is available for isolated testing. Custom installs can run
`scripts/status.ps1 -InstallDirectory` with their chosen folder.

If it stops, run `Status.cmd` and inspect `window-placer.log` beside the executable.
The bounded log records startup, normal exit, fatal errors and panics; it does not
record window titles or poll for activity. Check configuration errors before retrying
Setup. To uninstall, stop the process, remove `Window Placer.lnk` from `shell:startup`,
then remove your installed folder when you no longer need its configuration.
For a portable run, keep the executable and configuration together and launch
`window-placer.exe --apply`. Stop it in Task Manager or at logoff.

## Configuration

```text
# exe | x | y | width | height | optional title contains
Code.exe | 60 | 50 | 1400 | 900
notepad.exe | 100 | 80 | 1000 | 700
Wispr Flow.exe | 115 | 53 | 1690 | 955 | Wispr Flow
```

- Use executable file names, not paths; matching ignores ASCII case.
- Specify physical pixels relative to the window's current monitor.
- Keep width and height positive; targets shrink/shift into the monitor work area.
- Optionally require title text; the remainder after the fifth separator is the filter, including further `|` characters.
- Use unquoted fields, full-line `#` comments and UTF-8 (a BOM is accepted).
- Use one rule per executable and restart after editing; no configuration watcher runs.
- Pass `--config "C:\path\windows.conf"` for another file.

The included rules are starter sizes for common apps. Replace or remove entries
to suit your display. Sizes include invisible outer resize borders; Windows
display scaling does not enlarge these physical-pixel dimensions.

## Behavior

- Scan once at startup, then listen for show, title-change and destroy events.
- Wait three seconds after a supported main window becomes ready; place it once.
- Preserve later manual moves, including after hide/show or title changes.
- Keep the current monitor and request neither focus nor a stacking-order change.
- Skip minimized, maximized, fullscreen, cloaked, owned, tool and topmost windows.
- Wait for show/title events if initially hidden or untitled.
- Bound transient failures to three attempts; reconcile once after queue overflow.
- Allow one apply instance per session; preview instances move nothing.
- Re-place already open windows when the utility restarts.

One application event thread sleeps in the Windows message loop; Windows may
add helper threads. Configuration is parsed once using the standard library.
The only direct dependency is Microsoft's `windows-sys` bindings. The x64
release statically links its C runtime and needs no separate runtime installer.

## Build and test

Use Windows 10 version 1703 or later, Rust's x64 MSVC toolchain and its Windows linker.

```text
cargo fmt --check
cargo test --locked -- --test-threads=1
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
powershell -NoProfile -ExecutionPolicy Bypass -File tests/startup.ps1 -BinaryPath target/release/window-placer.exe
```

Find the executable in `target/release`. Put `windows.conf` beside it or pass
`--config`. Tests create/move only their own windows, including a child fixture
for cross-process WinEvents. The ignored fixture entry point runs through its
parent test. The startup test requires closing any running apply instance first. It
uses a temporary configuration that matches no real app, launches the actual shortcut
inside a disposable Windows job, and checks survival after that job closes, paths with
spaces, configuration preservation and failed-install isolation. CI runs these checks
on Windows.

## Limits

- Apps can reject a size, enforce minimum dimensions or resize themselves later; this utility does not fight them.
- Inaccessible/elevated windows may be left alone; administrator privileges are not requested.
- Protected windows stay skipped until the utility restarts.
- Geometry is tested; real mixed-DPI/multi-monitor hardware needs broader coverage.
- Use `--duration SECONDS` for a bounded run and `--help` for options.
- Exit codes: `0` normal, `1` configuration/setup/message-loop failure, `2` bad arguments or duplicate apply instance.
- Background apply mode never attaches to the launching terminal. Release builds have no console window; redirect output or use `cargo run` for diagnostics.

## Migration measurements

Read [From AutoHotkey polling to Rust events](docs/ahk-to-rust.md) for the measured
CPU/memory comparison, reproducible data, reliability fixes and limitations.

## History and license

The first commit captures the old AHK implementation. The second migrates to
Rust and archives it in [legacy/autohotkey](legacy/autohotkey). This is a
migration snapshot, not the original development timeline. Rust is maintained;
the archive is not installed or executed by the utility.

MIT licensed. Contributions and app compatibility reports are welcome.
