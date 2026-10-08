# Native manager core

Tasks 4–5 provide a Windows-native Rust library for commands, protected storage,
ownership and pure orchestration. Authority encoding, worker wiring and tray UI belong to Task 6.

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


## Lifecycle and Task 6 boundary

- Acquire `Arc<InstanceGuard>` with production scope `manager` and retain it for the
  worker lifetime. The non-inheritable `Global\VcXsrvWSL.manager.<TokenUser SID>`
  mutex is a handle-presence guard, not thread-affine mutex ownership. Existing
  object security is validated before duplicate-instance rejection.
- `ProtectedDirectory::open_or_create(path, guard)` requires a local absolute drive
  path, validates every component without following reparse points, and retains
  ancestor handles against rename/replacement. Owner plus the exact protected
  SID-only DACL are checked on opened files/directories. Files must be ordinary,
  single-link disk files. No unsafe existing ACL is repaired.
- `create_new`, `replace`, `read(name, limit)` and identity-checked `remove` share
  this primitive. Publication uses a random protected sibling file, complete write,
  flush and handle-based atomic rename under the guard. Payloads are limited to
  1 MiB; names cannot escape the private directory. A held `ProtectedFile` prevents
  writes/removal while the caller validates its bytes. Do not log those bytes.
- `ProtectedDirectory::inspect(path)` opens existing storage without a second
  manager guard. Reads get the same checks; all mutation methods reject this mode.
  This is the read-only `doctor --json` seam while the tray manager is running.
- `ServerBinding::new` binds executable, installation directory, storage directory,
  configured distro/user/display and existing authority identity. Task 6 must first
  validate authority contents against the canonical Task 7 binary-record contract.
  This layer deliberately does not generate, parse or rotate X cookies.
- If `ownership.json` exists, recover with existing authority before any creation
  or reset. `ServerOwner::recover` opens the recorded PID once, validates full
  creation FILETIME, process TokenUser SID, executable/file/installation identity,
  config and authority binding, then duplicates that same kernel handle for stop
  access. Missing authority, stale/malformed identity or unsafe storage is an error;
  never adopt a process by basename or kill it using a newly reopened PID.
- `ServerOwner::spawn` creates a persistent process directly using the shared native
  argv encoder, explicit executable, no inherited handles or manager pipes, and
  suspended startup. Identity and atomic ownership publication precede resume.
  Failed setup cleans up only its newly created never-resumed process. After resume,
  Drop, `detach` and ordinary manager exit only close handles. `stop_confirmed` checks
  current protected metadata, terminates the retained owned handle, verifies exit
  and removes that exact metadata file. Drop owner/binding before any explicit
  post-stop authority reset; the held authority must outlive the live server.
- Closing the manager leaves its server running. Windows sign-out/shutdown or an
  external launcher ending its own process group can still stop it. Startup requests
  breakaway from inherited jobs and fails if Windows denies creation; no weaker
  fallback is used. Residual job membership is recorded as `external_job`, with no
  claim that unknown ancestor jobs are harmless. No manager-created kill job owns
  the server. Use the normal desktop shortcut for desktop operation.

`Controller::dispatch(Action)` and `observe(Event)` are pure. The worker owns one
active operation and serializes native mutations; effects carry a generation and
one absolute 15-second deadline. Before each command/retry use `Effect::remaining`,
not a fresh 15-second timeout. Cancellation must invalidate late results and release
short-command work independently of the persistent server.

| Effect | Worker obligation |
|---|---|
| `EnsureServer` | Reuse/recover the owned handle; do not create a duplicate |
| `DiscoverRunning` | Host-only exact configured-distro membership query |
| `Bootstrap { allow_start }` | Only Launch may wake the configured distro; passive work must recheck running membership and fail honestly on shutdown races |
| `Probe` | Validate bounded v1 helper JSON, verb, configured display/port and success exit; only positive authenticated Unix-relay probe emits `Authenticated` |
| `Launch` | Invoke configured argv literally through the fixed helper; application output is not helper JSON |
| `StopOwnedServer` | Require explicit UI confirmation, then use retained ownership handle |
| `Detach` | Cancel short commands, reject new work, close ownership handles without killing the server |

Bootstrap success and socket/process presence never establish Ready. Refresh emits
no bootstrap. Ready represents the last authenticated observation and schedules no
guest polling; host-observed server/VM changes call `invalidate`.

Desktop-native acceptance (outside a restrictive test-host job): build the native
test binary and invoke it from an actual Explorer desktop launch:

```powershell
cargo test --manifest-path manager/Cargo.toml --locked --test lifecycle_native --no-run
# Use the emitted executable path:
# lifecycle_native-<hash>.exe --include-ignored --test-threads=1 --nocapture
```

The two lifetime tests and two job diagnostics are ignored in ordinary harness runs.
The lifetime fixture tests normal process exit and `process::exit` without Rust
destructors; it does not inject a Windows exception or claim cross-session survival.
