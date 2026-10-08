# Native manager core

Tasks 4–6 provide a Windows-native manager for commands, protected storage, ownership, authenticated helper orchestration and a native tray UI.

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

## Native tray and isolated diagnostics (Task 6)

The product binary is `vcxsrv-wsl-manager.exe`. `--tray` (also the default) creates a
hidden native top-level window, starts its serialized worker and passively attempts
to start/recover the configured companion. `vcxsrv.exe` is beside the manager by
default. Missing companion/helper paths produce fixed actionable errors; a running
manager or server process is never sufficient for Ready.

```powershell
# Parent directory must already exist; the final private directory must be new,
# or already have the exact current-user-only protected security.
.\vcxsrv-wsl-manager.exe --tray --state-dir C:\Users\walnu\Documents\VcXsrvTest
.\vcxsrv-wsl-manager.exe doctor --json --state-dir C:\Users\walnu\Documents\VcXsrvTest
.\vcxsrv-wsl-manager.exe recover-stopped --state-dir C:\Users\walnu\Documents\VcXsrvTest
```

The default state directory is Windows KnownFolder LocalAppData + `VcXsrvWSL`.
`--state-dir ABSOLUTE_DIR` isolates config, authority, ownership and logs from
installed state. `--config ABSOLUTE_FILE` selects another config, whose parent and
file receive the same protected storage checks. An explicit missing/unsafe config
fails instead of silently using defaults. The first tray run creates protected
`config.json` with the existing defaults; edit it through Open config and restart
the manager. `doctor --json` is read-only, works alongside the tray, creates no
state and reports `last_authenticated_state: null`: it does not run a guest probe
or infer Ready from host/configuration observations. Protected-file validation
never silently repairs an existing directory or file. One production manager per
actual TokenUser SID across sessions still applies even with isolated paths.

Tray actions: Start server, Launch Emacs, Refresh connection, Diagnostics, Open
config, Open logs, Start manager at sign-in, Recover stopped server record, Stop
server and Exit manager. Stop displays a separate default-No confirmation. Exit
cancels short helpers, rejects new actions and detaches; it never means Stop. The
window does not wait for its worker on exit. Startup is off until explicitly
enabled through the tray; it changes only the current-user `VcXsrvWSL` Run entry,
retains explicit config/state arguments and refuses an entry from another install.

Authority uses BCryptGenRandom and one canonical FamilyWild Xauthority record,
MIT-MAGIC-COOKIE-1, configured display0..99 and exactly16 cookie bytes. Parsing
rejects truncation, noncanonical selectors, wrong family/address/name/data lengths,
additional records and trailing bytes. The authority shares Task5's protected
atomic storage, remains locked throughout owned server lifetime and has no
Debug/Display implementation. An existing ownership record requires an existing
valid credential; no crash/restart recovery recreates or rotates it.

The literal system `wsl.exe` argv selects only the configured distro/user and
executes `/run/current-system/sw/bin/vcxsrv-wsl` with `bootstrap`, `probe` or
`launch -- PROGRAM ARG...`. Bootstrap sends bounded binary stdin then EOF; launch
arguments are never shell text. Strict bounded v1 success JSON must match verb,
display and derived port. Bootstrap cannot become Ready. Only the authenticated
normal Unix-relay probe establishes Ready, which is labelled the last authenticated
observation. Launch completion is distinct from that probe; application output is
never parsed as helper JSON or logged. Every connection/launch effect shares one
absolute15s attempt deadline; timeout has no success claim and does not automatically
retry Launch. Killing/timing out Windows wsl.exe does not prove an already-started
guest application was undone; the manager never terminates WSL or arbitrary guest
processes to compensate.

A bounded typed queue accepts one pending operation, coalesces duplicate clicks and
serializes lifecycle mutation. The idle worker checks the retained server handle.
While Waiting/Ready, selected-distro membership is a host-only list operation with
5s-to30s backoff, including at most5s per query. An observed guest stop invalidates
Ready; a later appearance may connect/bootstrap passively, without waking it.
Ready schedules no guest probes. A same-name stop/restart entirely between host
observations cannot be detected; explicit Launch/Refresh performs a fresh probe.
TaskbarCreated restores both the tray icon and its notification version without
restarting Explorer.

### Explicit stopped-record recovery

Ordinary Start retains the existing fail-closed stale-metadata rule. After a Windows
reboot or verified server exit, **Recover stopped server record** or `recover-stopped`
can retire only an unchanged protected ownership record after positive native stopped
evidence. It requires current SID, directory/config scope and valid unchanged
canonical authority. A matching live process or unknown/query/snapshot failure is
refused. A creation-time mismatch or checked complete snapshot proving the recorded
PID absent establishes only that the original process is gone; it never authorizes
adoption or termination of a different process. A syntactically valid old executable
path may be absent after upgrade; it is never opened/executed by retirement. Authority
bytes and file identity stay unchanged. Start server is a separate subsequent action.

`remove_if_unchanged` protects against both replacing a classified metadata file and
rewriting its bytes without changing file identity. Recovery holds the production
instance guard and serializes with live lifecycle work. A missing state directory
or missing record causes no credential/config creation. `recover-stopped` refuses
while the tray's production instance guard is active; use the tray action instead.

### Verification scope

Default `cargo test --offline` includes byte-level authority/helper/queue/controller
checks, native stopped/live/absent-process recovery, protected compare/remove, and
an actual isolated manager executable tray smoke. That smoke verifies top-level
window responsiveness, missing companion Error, actual duplicate-instance rejection,
concurrent read-only doctor, synthetic TaskbarCreated restoration and prompt exit;
it invokes no WSL and creates no authority/server. The desktop-only Task5 persistent
owner fixtures remain explicitly ignored in normal harness runs. Full guest auth,
owned-server tray detach/crash, sign-out, sleep and application/WSL acceptance belong
to the separately authorized machine acceptance. A synthetic creation mismatch test
is not evidence of observed Windows PID reuse.
