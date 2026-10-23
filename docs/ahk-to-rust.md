# From AutoHotkey polling to Rust events

Historical report: v0.1.1 measurements and v0.1.2 startup validation. For current
behavior, which supersedes the original event description, see the [README](../README.md).

Window Placer gives applications a useful starting size and position, then leaves
later manual adjustments alone. Replacing the old AutoHotkey setup with a small
native Rust program reduced measured background CPU and memory use. The main
design improvement was replacing repeated searches with Windows events.

## What changed

The archived [AutoHotkey implementation](../legacy/autohotkey) queried eleven apps
once per second at idle: 39,600 queries per hour. It coordinated a launcher and
separate app scripts. The Rust implementation loads one configuration, scans once,
then waits for show, title-change and destroy events. It schedules placement after
a window becomes ready and remembers that the window has already been handled.

There is one application event thread, no polling loop, no async runtime and no
resident PowerShell helper. Windows can create additional system helper threads.
Rules use executable names and optional title filters. Protected windows are
skipped, and later manual moves are preserved. This implements the original intent
rather than reproducing every old script behavior.

## Measurements

These samples were taken on the same Windows PC on October 21, 2026, at different
times. They are short idle observations, not a simultaneous controlled benchmark.
The Rust measurements below are from v0.1.1; the startup improvements ship in v0.1.2.

The table compares the resident window-placement process in each setup. It excludes
unrelated AHK scripts and short-lived launch-time child processes.

| Metric | AutoHotkey launcher | Rust v0.1.1 |
| --- | ---: | ---: |
| Observation time | 30.61 seconds | 20.91 seconds |
| Process CPU time gained | 0.390625 seconds | 0 seconds detected |
| Average CPU, one logical core | 1.28% | Below sample resolution |
| Private committed memory | 4.34 MiB | 1.03–1.13 MiB |
| Working set | 15.94 MiB | 6.90–6.95 MiB |

That is about **3.2–3.3 MiB less private commit (75%)** and **9 MiB less working
set (56%)**. These are different memory measures and must not be added together.
Zero detected CPU growth does not mean the program never uses CPU.

The [AHK samples](measurements/ahk-idle.csv) and
[Rust samples](measurements/rust-0.1.1-idle.csv) retain elapsed time, cumulative CPU
growth, private bytes and working-set bytes. CPU percentage is the last CPU delta
divided by elapsed wall time, multiplied by 100; it is not normalized across all
logical processors. MiB means bytes divided by 1,048,576. Samples omit personal
paths, process IDs and window titles.

Both the language and architecture changed, so these results do not isolate a
benefit from Rust alone. An event-driven AHK implementation could also avoid the
polling cost. The absolute memory savings are modest; the simpler process model
and idle behavior are the more useful improvements.

## Making startup dependable

An earlier installed process unexpectedly disappeared. Its exact exit cause was
not captured, so it would be misleading to attribute it to one proven cause.
We found fragile deployment boundaries and removed them: background apply mode
does not attach to a terminal, and installation uses a stable per-user Applications
folder rather than relying on packaged-host AppData behavior.

The installer now starts the actual saved login shortcut through a short-lived,
hidden launcher that explicitly breaks away from the launching job. Windows
normally associates child processes with their parent's job; job cleanup can
terminate those children. See Microsoft's [job-object documentation](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)
and [process creation flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags).
If Windows disallows breakaway, setup reports failure rather than silently
claiming success. The helper exits after launch; it is not a background supervisor.

`Setup.cmd` installs, preserves existing settings, creates the per-user Startup
shortcut, launches it and verifies the process. `Status.cmd` checks the saved
target and arguments and displays recent bounded lifecycle diagnostics. Invalid
configuration is rejected before setup stops a healthy installed process.

## Validation and limits

The v0.1.2 checks cover 43 Rust tests, formatting, warnings-as-errors lint and a
release build. A Windows integration test launches a real shortcut with paths
containing spaces, closes a kill-on-close parent job, verifies the app survives,
checks preserved settings and rejects invalid setup without stopping the listener.
The real installed Startup shortcut was also launched successfully after its
launcher exited. Automated placement tests move only their own windows.

We did not reboot the machine for this validation. Login startup depends on Windows
keeping the entry enabled. This is not a service or watchdog: explicit termination,
OS shutdown and an unrecoverable process failure can still stop it. Logs make exits
inspectable. Real mixed-DPI hardware and longer-running desktop workloads remain
areas for broader compatibility testing.
