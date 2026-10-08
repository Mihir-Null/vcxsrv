# Native Windows transport tests

Run on Windows 11 x64 with Visual Studio 2022 BuildTools and SDK 10.0.26100.0:

```powershell
pwsh -NoProfile -File tests/native/all.ps1 -XcbBuildRoot C:/path/to/built-source-snapshot
```

`XcbBuildRoot` must contain built Release x64 XCB objects, libXau and pthreads.
The live fixture relinks those existing objects into `.native-tests/libxcb.dll`
with this checkout's export definition; it never changes the supplied snapshot.
Build outputs and logs go in ignored `.native-tests/`.

- `run.ps1`: real local sockets, private current-user ACL, checked socket narrowing,
  binary I/O, 80 simultaneous clients, restart and failed-bind/path cleanup.
- `run.ps1 -Test winpoll`: actual production readiness adapter, 80 descriptors,
  accept/read/write/EOF, idle deadline, cancellation, partial-read rearming and
  write backpressure. Actual Hyper-V loopback and AF_UNIX providers are exercised
  together when Windows permits binding; unavailability is explicitly reported.
- `xtrans.ps1`: real included server xtrans code, normal auth-capable accepts,
  metadata, listener reset/close and listener-array allocation failure cleanup.
- `winxcb.ps1`: real socket connector with a narrow XCB boundary fixture to force
  allocation and setup failures; verifies reference transfer and screen bounds.
- `winxcb-live.ps1`: actual bundled XCB authentication setup bytes and success /
  rejection handling; fixtures use disposable non-secret test cookie bytes.
- `managed-profile.ps1`: production option observer and final validation,
  both flag orders, generic invocation isolation, and real binary authority parsing.
- `managed-auth.ps1 -BuildRoot ...`: actual production ClientAuthorized gate,
  managed AF_HYPERV/AF_UNIX/TCP peer metadata, host/NOXAUTH bypass denial,
  successful authorization and preserved generic behavior. Its authorization,
  address and audit dependencies are controlled; full server setup remains live
  acceptance through `tests/acceptance/Test-Server.ps1` and `x11-probe.py`.
- `compile-units.ps1`: compile all affected production translation units.

No test starts WSL, changes services/registry/firewall, or creates a TCP listener.
These fixtures do not establish full-server protocol operation, WM/clipboard
operation, or access denial under another Windows user's token.

- `winadmission.ps1`: links the production native admission module to real HWNDs.
  Checks the first real `EVENT_OBJECT_SHOW` via `SetWinEventHook`, hidden preparation,
  real cross-thread owner callbacks, queued completion, stale XID/token rejection,
  duplicate/reentrant completion, 50 surviving-HWND remaps, destroyed-window identity,
  initial minimized/maximized states, and owned/tool/NOACTIVATE windows. The fixture's
  supplied native hints are test inputs; actual X hint parsing and automatic Leopard
  admission require the full server acceptance run.
- `admission-trace.ps1 -Trace <ordinary-window-event-log>`: rejects an ordinary
  client's real SHOW events with TOOLWINDOW or without APPWINDOW. The preserved
  `vcxsrv-build-tools/test-window-events2.log` intentionally fails this assertion.

- `discovery.ps1`: links the real Unicode parser, hidden Win32 runner, query and
  lifecycle module. Covers UTF-16LE BOM/no BOM and CRLF, exact Unicode/spaces
  selection, malformed/truncated/oversized output, invalid GUIDs, stopped/missing
  guests, WSL1/unknown version rejection, shared discovery deadlines, and stale
  pending cancellation. Real fixture children prove literal argv, original wide
  command-line recovery, explicit handle inheritance, separate stderr, bounded
  output, timeout and descendant-held-pipe cleanup. Fake WSL runner data exercises
  the real query/lifecycle policy without starting or mutating any WSL guest.
  Host registration Version is a conservative compatibility dependency; full
  server/WSL runtime, localized hosts and coordinated restart/idle tests remain
  separate acceptance work.

- `winadmission-state.ps1`: invokes the same production `ApplyState` callback used
  by the WM worker, checking iconic/zoom remaps of a surviving HWND with unchanged
  recorded state, plus unchanged-state no-op behavior outside pending admission.
