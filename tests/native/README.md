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
- `compile-units.ps1`: compile all affected production translation units.

No test starts WSL, changes services/registry/firewall, or creates a TCP listener.
These fixtures do not establish full-server authorization policy, WM/clipboard
operation, or access denial under another Windows user's token.
