# VcXsrv WSL manager and McQueenix integration

Date: 2026-10-07
Status: Design and implementation plan approved; subagent execution underway. User confirmed the rebuilt NixOS WSL is online on 2026-10-07. Preserve existing Emacs sessions during tests.

## Outcome

One per-user Windows installation supplies a small native tray manager and a reproducibly built VcXsrv. On McQueenix, a declarative NixOS/Home Manager module supplies the X11-to-VSOCK relay and environment used by kittymacs. Launching Emacs becomes one action. LeopardWM sees and manages individual native X11 windows. WSL restarts do not require hand-editing VM IDs or display addresses.

The initial supported target is Windows 11 x64, Store WSL2, the NixOS distribution, user Empty, and the mcqueenix host. Distro, user, display number and launcher commands remain configurable. Native Wayland, audio, SSH forwarding and a general application store/browser are outside the first release.

## Evidence and source baseline

- User fork: https://github.com/Mihir-Null/vcxsrv
- Initially only pages at 3403dd56b9ac8276ada4fdaf0d037e1f9b33f957 was copied. At the user's request, master, released, feature/raw-input-mouse, feature/vsock-wsl2 and feature/vsock-wsl2-pr were subsequently imported from their upstream sources. pages remains the GitHub default branch.
- Implementation baseline: vivimillin/vcxsrv feature/vsock-wsl2 at 5cd992072b1b94d7d6312f0dab762638ec4ebbe4, also the fork.2 release commit. This includes the author's mouse fixes as well as VSOCK work; retain provenance and do not claim the baseline contains only transport changes.
- Upstream VSOCK submission: https://github.com/marchaesen/vcxsrv/pull/81 (six commits; 11 files; not merged or independently reviewed at inspection).
- Existing server performs VM discovery and rebinding. It uses process-presence polling, selects a running distro, and invokes wslinfo. Its parser assumes English Running and does not support names containing spaces reliably.
- Rust/MSVC tools are installed. The complete legacy VcXsrv build dependencies are not confirmed present. The old Docker recipe needs Windows containers and must not be assumed to work in NixOS's Linux Docker.
- GWSL source license is nonstandard. Implement manager code independently; do not copy GWSL code or assets.

## Alternatives and selection

1. Recommended: small VcXsrv integration patch plus an independent native Rust manager and declarative Linux module. This preserves existing server code and avoids inheriting a large GUI dependency stack.
2. Wrapper only around upstream -vmid: simpler server diff, but restarts may require restarting the X server, disconnecting clients; duplicates lifecycle logic already present in the fork.
3. Fork the whole GWSL manager: wider surface, uncertain reuse terms, old dependencies and imperative shell editing inappropriate for NixOS.

## Repository structure

Keep server changes in the existing X11/xtrans and xorg-server trees. Add manager/ for a standalone Rust package, integration/nixos/ for the NixOS module and Home Manager integration, packaging/ for per-user packaging and installation scripts, and docs/ for operation, recovery and build instructions. Carry changes on a local development branch based on the implementation commit; preserve pages. No force pushes or changes to GitHub default branch.

## Server behavior

Provide an explicit managed WSL profile instead of changing generic VcXsrv behavior globally. The profile uses multiwindow mode, clipboard integration, automatic WSL VSOCK binding, and explicitly disables TCP/IPv4/IPv6 listeners. It never silently falls back to TCP. Keep existing command-line options for generic X-server users.

Support selecting the configured distro for VM discovery, bounded Unicode-safe process creation, validated arguments and diagnostics for old WSL/missing wslinfo. Fix demonstrated parser/process problems rather than expanding the transport unnecessarily. Prefer documented machine-readable running-distro queries and configured distro selection over parsing localized verbose output. Keep no guest processes running in the steady state solely for discovery.

Authentication is a release requirement: the managed profile must require an X authority cookie and must not admit peers solely because Hyper-V connections classify as FamilyLocal. Audit existing -auth and host-based fallback first; add a narrowly scoped strict option only where the current implementation cannot enforce this. Validate missing and incorrect cookies are rejected. Bind the exact WSL VM, but do not describe VM binding as per-distro or per-user isolation.

## Manager

Use Rust with Win32 APIs and a native tray menu, with no browser/WebView or Python runtime. Reuse the installed MSVC toolchain. Keep configuration and orchestration separate from tray code for testing.

Initial actions: status/diagnostics, start server, launch Emacs, open configuration/logs, enable/disable login startup, and exit. Display Waiting for WSL, Connecting, Ready, or Error based on actual process/protocol checks, not just a running server process. Long operations run off the GUI message thread and have bounded timeouts.

Versioned configuration stores distro (NixOS), user (Empty), display (default 1), companion server path, and a launcher executable plus argument array. Launch Emacs with emacsclient -c -n through a fixed helper that supplies the display and authority environment. Avoid shell command concatenation. Launch is allowed to start the configured distro; passive status monitoring must not wake a stopped distro.

Default display 1 / VSOCK port 106001 avoids taking WSLg's display 0 during initial acceptance. Make display selection explicit and detect occupied sockets. This can remain display 1 when WSLg is disabled; display 0 has no inherent functional advantage.

Run one manager instance per Windows user. Track only child processes owned by this installation; never kill by executable name or stop another user's X server. A server restart or manager exit that terminates the server warns that connected GUI clients can be lost. Ordinary manager UI closure should not silently destroy the display server.

## Credentials and Linux integration

Generate the X cookie with a cryptographic RNG, never log it, and store authority files with user-restricted Windows ACLs and Linux mode 0600. Use a bounded stdin-based bootstrap to place the cookie for the selected Linux user when launching/connecting; do not put secrets in process arguments, the repository or the Nix store. Preserve one cookie for the life of the server; rotation is coordinated with an explicit server restart.

The NixOS module supplies a managed relay service with restart handling and appropriate socket ownership/permissions, plus a runtime authority location. It exposes display and port options shared with the manager configuration. Use one service, never one socat process per shell. A per-user relay endpoint is preferred where compatible with X11 clients; if the conventional /tmp/.X11-unix/X1 endpoint is used, create the parent safely and restrict the socket to the configured user. Direct VSOCK access must still require the cookie.

The Home Manager integration selects pkgs.emacs through programs.kittymacs.package and configures the daemon's DISPLAY/XAUTHORITY and service dependency/readiness behavior. Launch must wait for a real authenticated X handshake, not just a socket file. Socket readiness alone is insufficient because the Windows server may be waiting for the VM or credentials.

Keep NixOS changes in the authoritative Org source in EmptyNet and tangle generated Nix files. Do not modify the user's currently rebuilding checkout until that rebuild has finished and current work has been inspected. Do not restart the existing Emacs daemon with unsaved buffers. A restart is required to change the executable; a daemon startup alone does not prove GUI connectivity.

## Installer and builds

Build a native manager executable and a portable bundle with a per-user install/uninstall path, launcher shortcut and opt-in startup registration. Installation is idempotent, validates companion files, preserves user configuration and provides rollback. Use a clearly separate product name/path so it does not overwrite an unrelated VcXsrv installation.

Keep the server build pinned and record commit identities, compiler/build dependencies, bundled files, hashes and required license/source notices. Reproduce the server build using the existing MSVC build path with a documented dependency bootstrap. Do not silently use an upstream prebuilt binary and call it a build of this fork. If a prebuilt binary is temporarily used for a diagnostic, label it explicitly and keep source-build completion outstanding.

No new Windows service, firewall exception, Hyper-V group membership or elevated runtime should be needed. Do not change WSLg global settings, reboot or shut down WSL while the user's upgrade is running. After successful coexistence tests, document the explicit WSLg cutover and restoration steps.

## Validation

1. Tests for configuration validation, Windows argument handling (spaces/Unicode), process timeouts, stopped-WSL behavior, authority serialization, ownership/lifecycle and status transitions.
2. Build/lint the manager; build the pinned server; package/install into an isolated test directory and verify uninstall preserves unrelated state.
3. Negative live authentication tests: missing/wrong cookie rejected over VSOCK, valid cookie accepted. Verify no TCP listeners. Do not substitute a fake test for these properties.
4. When the user confirms new WSL is live, inspect the actual package/service/configuration before activation. Open a separate test Emacs frame using the X11 build.
5. Verify LeopardWM sees the test window and correctly tiles/resizes/focuses it, switches workspaces, and handles menus/dialogs. Check clipboard and display scaling.
6. Exercise server-before-WSL and WSL-before-server startup. Coordinate disruptive WSL restart and sleep/resume tests with the user; record passed, failed and not-run separately. WSL shutdown ends guest processes; listener recovery does not preserve those processes.
7. Verify the tray manager does not prevent WSL idling and recovers on the next explicit launch.

## Completion evidence

Deliver source changes, reproducible build and packaging commands, the built manager and server artifacts, the EmptyNet integration change, rollback instructions and an honest test report. Keep unverified claims about hardening and tiler compatibility out of product copy. Do not publish a release or purchase signing credentials as part of this work.

## Approved execution updates

- User approved subagent-driven execution, adding a Windows-local transport for internal helpers, and lifted the WSL rebuild hold on 2026-10-07.
- Linux integration will use a private runtime AF_UNIX socket with DISPLAY=unix:/absolute/socket. This format was verified against the installed Xlib with a diagnostic setup exchange. Xauthority client lookup uses display number 0 for this path format; preserve server display 1 and port 106001 and rewrite only the guest authority record selector, not its cookie.
- Windows internal transport must also require a cookie; no host-based bypass on AF_UNIX. Remove existing WINDBG cookie-content logging in winauth.c. Pair Winsock lifetime correctly with bundled xcb's WSAStartup/WSACleanup behavior.
