# Native manager core

Task 4 provides a Windows-native Rust library. Server ownership, credentials storage,
controller state and tray UI belong to subsequent tasks.

- `Config::defaults_for(manager_exe)` locates `vcxsrv.exe` next to the absolute manager
  executable. `Config::from_json` applies those defaults to omitted fields, requires
  schema version 1, rejects unknown fields, and validates the result. Directly
  constructed/deserialized `Config` values must call `validate` before use.
- `ProcessRunner::run(&spec, stdin)` is for disposable host commands only. It creates
  a separate kill-on-close job, starts the child suspended, assigns the job, then
  resumes. A restricted handle list passes only stdin/stdout/stderr. Never use it
  to own the persistent display server.
- Process I/O uses current-user-only named pipes with first-instance and remote-client
  rejection. Parent endpoints are overlapped and non-inheritable. Stdin closes after
  the complete write. Stdin is capped at 1 MiB; stdout plus stderr share the configured
  output cap (default 1 MiB). The monotonic timeout covers stdin, output and child wait,
  including descendants that retain output pipes after the original child exits.
- Windows cancellation completes asynchronously. Pending I/O buffers/handles are
  retained by a cleanup thread until its completion event signals; the caller never
  joins that thread. In the exceptional inability to create that cleanup thread or
  failed completion wait, pending resources are retained rather than freed while
  the kernel can access them. Native OS process creation/setup calls are synchronous
  and cannot themselves be forcibly interrupted by this deadline; use trusted local
  executables. No guest PID killing or WSL termination is used.
- Argument encoding follows standard minimal Windows quoting. Simple switches remain
  unquoted because installed WSL treats always-quoted `--list` as a guest command.
  Unicode, quotes, trailing backslashes, empty strings and metacharacters remain
  literal arguments. Executables must be absolute and no shell is invoked.
- `WslClient::running_distros` obtains the trusted system directory from Win32 and runs
  only `wsl.exe --list --running --quiet`. It preserves exact names and decodes strict
  UTF-16LE/UTF-8 without localized status parsing. `running_distros_with_timeout`
  allows a future controller to pass the remaining connection-attempt budget.
- Captured output is untrusted. Debug output for specs, captured output and config
  deliberately omits string/byte payloads; failures report fixed kinds/native codes
  without captured stderr or stdin. Callers must not log the raw output.

```powershell
cargo test --manifest-path manager/Cargo.toml --locked
cargo clippy --manifest-path manager/Cargo.toml --locked --all-targets -- -D warnings
```

The explicit installed-WSL smoke is excluded from ordinary isolated fixture runs:

```powershell
cargo test --manifest-path manager/Cargo.toml --locked --test wsl installed_wsl_running_list -- --ignored --exact
```

That smoke expects the authorized acceptance host to have NixOS already running.
It only lists host membership; it never selects a distro or executes a guest command.
`process-fixture` is a test executable, not the final manager product.
