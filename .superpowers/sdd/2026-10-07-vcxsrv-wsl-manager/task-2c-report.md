# Task 2c: managed native initial admission

Base: `68ad567b21385b0452b17e653942e9f43adfc019` (verified before edits).

## Result and lifecycle

- Only managed-profile, mapped, top-level InputOutput windows without override-redirect use two-phase initial mapping.
- Owner creates the hidden native window, assigns a process-unique map token and copied XID, applies shared native baseline preparation, then queues `WM_WM_MAP_PREPARE`.
- Worker performs existing XCB name/hint/icon/state queries. Native mutations execute through synchronous owner callbacks carrying a copied HWND/XID/token identity. Callback payloads remain valid until SendMessage returns; they contain no borrowed X WindowPtr. Callbacks must never query XCB or wait for the worker.
- This routing also covers subsequent native property/state updates in the managed profile, so they cannot bypass a pending initial show. The native property marker survives completion to distinguish later show/restore callbacks from initial preparation.
- After all initial work, worker posts `WM_WIN_ADMISSION_READY` with scalar XID/token. Owner validates live/mapped/top-level/InputOutput/non-override state, applies shape work, consumes pending state before synchronous show callbacks, then performs the first real ShowWindow using the final normal/minimized/maximized state.
- No fake accessibility event, manager refresh, polling, or Leopard change. Ordinary generic and override-redirect dispatch retain the existing path. Transient native owners and hint-selected TOOLWINDOW styles are preserved.
- Unmap/destroy/reparent-out invalidate admission; a new map uses a new token, including when HWND or XID is reused. Initial allocation/queue failure is explicit/fatal. Ready-post failure logs once for that attempt and uses one synchronous, identically validated delivery instead of silently leaving a live window invisible.

## Files and seams

- `xorg-server/hw/xwin/winadmission.[ch]`: Win32-only production identity/visibility seam, shared native baseline style preparation, owner callback dispatch.
- `winmultiwindowwindow.c`: managed initial map start and cancellation.
- `winmultiwindowwndproc.c`: owner live checks, native callback/ready messages, shared baseline preparation, skip duplicate MAP_MANAGED/reset, avoid foreground activation for initial iconic state.
- `winmultiwindowwm.c`: XCB query/native mutation separation. The existing hint style policy is extracted rather than copied. Motif no-decoration heuristic remains before configuration overrides. WS_VISIBLE is suppressed while initial admission is pending. WM_HINTS initial iconic state is honored before first show.
- `winmultiwindowicons.[ch]`: split existing icon loading from native application, retaining generic wrapper and disposal of icons whose identity expired.
- `makefile`, `meson.build`: add native module. Native compile gate now includes all affected units with the actual server's XWIN_GLX_WINDOWS definition.
- While preserving state publication, corrected two pre-existing `UpdateState` memory errors directly in the exercised path: compare property atoms before freeing the reply, and pass the actual atom buffer rather than its pointer's address to XCB.

Task 3 should consume the managed server lifecycle normally. There is no manager-side refresh API. Native internal protocol is QUERY/APPLY/READY; per-map tokens are private, owner-thread state and never a persistent manager identifier.

## Verification

- Red regression: new ordinary-client event trace assertion failed on the preserved real baseline: `8002 ... ex=80 ... title=VcXsrv X`, before APPWINDOW/title hints.
- Passed `tests/native/winadmission.ps1`: actual production gate with real Windows windows and real `SetWinEventHook(EVENT_OBJECT_SHOW)`. Normal first SHOW has final title and APPWINDOW/no TOOLWINDOW; pending state was already consumed. Real worker-thread SendMessage and PostMessage completion exercised.
- Passed stale XID/token, false-live rejection, duplicate/reentrant completion, 50 rapid surviving-HWND hide/cancel/remap/minimize cycles, late posted completion after remap, destroyed/new identity, initial maximize, owned/transient baseline, TOOLWINDOW/NOACTIVATE retention.
- Passed complete `tests/native/all.ps1`: local lifecycle/ACL/I/O, mixed AF_UNIX/AF_HYPERV polling, Xtrans, connector boundaries, real bundled XCB authentication setup, managed profile/VSOCK policy, strict authority framing, actual ClientAuthorized, new native admission fixture, all affected production translation units. Existing compiler warnings remain; no test failure or compile error.
- Full server link and actual new Emacs/Leopard automatic admission, focus/geometry, real X override-redirect/InputOnly/skip-taskbar/fullscreen variants remain controller-owned acceptance. The native fixture does not claim to establish those X11 integration behaviors.
- Build-ready source list sent to controller after successful compilation. One focused self-review completed; fixed explicit unsigned XID widening and direct relative include paths. No server/client/WSL/user daemon was stopped or changed by this implementer.

Logs (ignored local evidence): `.native-tests/admission-compile.log`, `.native-tests/admission-all.log`. Preserved baseline: `C:/Users/walnu/source/vcxsrv-build-tools/test-window-events2.log`.


## Review fix round 1: unchanged recorded state on a new admission

Base: `b0b81f730160f66c4d3ab5ca7f2050d4c43cfd10` (includes Task 3, preserved).

- Verified the P2: `winAdmissionBegin` resets the per-map show command while a
  surviving HWND retains `WIN_STATE_PROP`; `ApplyState` returned on unchanged state
  before supplying the new admission's iconic/zoom command.
- Extracted the existing small owner callback into shared production
  `winnativestate.h`, included by `winmultiwindowwm.c` and the regression fixture.
  The test exercises that actual callback, rather than directly setting the
  admission command or duplicating its policy.
- Changed only the early-return condition: unchanged state returns immediately
  when no admission is pending. During a pending map the requested show command
  is captured even if state is unchanged. The recorded native property is only
  set when it changes; `NativeState.previous` and `UpdateState`'s early return
  remain unchanged, preserving redundant X property publication suppression.
- Red: actual callback fixture initially passed initial iconic/zoom admission but
  failed both same-state remaps (`iconic=0 zoomed=0 visible=1`). Green: both remaps
  pass, and same-state calls outside pending admission (including generic native
  windows) remain no-ops. Disposable TOOLWINDOW fixture windows avoid interference
  from the desktop tiler. Explicit CHECK assertions execute in release builds.
- Passed focused `winadmission-state.ps1`, existing `winadmission.ps1`, and the
  affected `winmultiwindowwm.c` compilation. Compiler emitted only existing
  warnings; the focused fixture compiled cleanly. The unrelated full suite was
  intentionally not repeated for this scoped correction.
- Frozen build files: `winmultiwindowwm.c`, `winnativestate.h`. No build-list
  change is needed for the shared header. No live server, Emacs, WSL, or Task 3
  implementation was modified. Controller owns full build and scoped review.

Evidence: `.native-tests/admission-fix1-red.log`, `admission-fix1-green.log`,
`admission-fix1-native.log`, and `admission-fix1-compile.log`.
