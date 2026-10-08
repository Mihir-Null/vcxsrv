# Declarative managed VcXsrv guest

This module supplies one private Unix-to-VSOCK relay and a fixed binary-stdin
helper. Home Manager extends kittymacs's existing X11 Emacs daemon; launch is
an authenticated readiness boundary. Bootstrap success alone never means Ready.

## Composition

Import `default.nix` beside Home Manager's NixOS module, then configure:

```nix
services.vcxsrvWsl = {
  enable = true;
  user = "Empty";
  display = 1;
  port = 106001; # default: 106000 + display
};
home-manager.users.Empty.vcxsrvWsl.enable = true;
```

The target home must already import/enable kittymacs and its daemon. The module
selects `pkgs.emacs`, derives its client executable from that package, and rejects
missing accounts/homes, disabled integration, mismatched display or invalid port.
NixOS emits `vcxsrv-wsl-relay.service` once with `ConditionUser=Empty`. Home Manager
adds only Wants/After and scoped environment to the existing `emacs.service`.
It preserves `X-RestartIfChanged=false`, permits headless startup, and does not
bind Emacs's lifetime to the relay. The existing HM login shell remains; later
coordinated daemon replacement must verify its effective environment.

The installed `/run/current-system/sw/bin/vcxsrv-wsl` is a Python executable with
a pinned shebang and immutable nonsecret config; Windows invokes it with WSL
`--exec` and separate argv. No shell wraps the bootstrap/probe/launch boundary.
Module installation or switching is a separate operation from source review.

## Protocol v1

| Invocation | Input | Successful result | Bound |
|---|---|---|---|
| `bootstrap` | One binary Xauthority record followed by EOF | `bootstrapped`, exit0 | Input2s, lock/service5s |
| `probe` | EOF, no bytes | `ready`, exit0 after full authenticated setup | Input2s, handshake2s |
| `launch -- PROGRAM ARG...` | EOF, no bytes | Exact target exec/output/exit semantics | Input2s, readiness10s |

Bootstrap/probe emit one UTF-8 JSON line below1024 bytes, e.g.
`{"version":1,"verb":"probe","result":"ready","display":1,"port":106001}`.
Errors add `code` with `result=error` and the configured display/port. Only fixed
safe codes enter stdout/stderr; exceptions, cookies and X-server reason payloads
are never reflected. There is no trace/debug payload switch.

| Exit | Codes |
|---|---|
| 2 | `invalid_arguments`, `invalid_record` |
| 3 | `account_mismatch`, `config_mismatch` |
| 4 | `unsafe_path` |
| 5 | `relay_unavailable` |
| 6 | `credential_missing`, `credential_conflict` |
| 7 | `x11_rejected`, `x11_protocol` |
| 8 | `deadline` |

The canonical input is FamilyWild65535, empty address, canonical ASCII server
display0..99, `MIT-MAGIC-COOKIE-1`, and exactly16 cookie bytes. All integers and
lengths are big-endian u16; limit64 bytes, exactly one record, no trailing bytes.
The guest rewrites only the display selector to ASCII0. Xlib/XCB authority lookup
for `DISPLAY=unix:/absolute/socket` uses selector0 even when server display is1.
The cookie itself remains identical to the Windows server cookie.

The helper resolves the account UID at runtime and accepts only canonical
`/run/user/<effective-uid>`, rejecting mismatched XDG_RUNTIME_DIR or symlinked,
misowned or permissive components. Private `vcxsrv-wsl` is0700; authority/lock
are0600 regular single-link files opened with dirfds and O_NOFOLLOW. A bounded
flock serializes bootstrap/relay updates. Authority writes use random exclusive
temporary files, fsync and atomic replacement. A matching record is reused;
a differing cookie returns `credential_conflict` without rotation. Bootstrap
releases the lock before starting the declarative relay through systemctl.

Launch sets `DISPLAY=unix:<runtime>/vcxsrv-wsl/X<display>` and
`XAUTHORITY=<runtime>/vcxsrv-wsl/authority`, removes inherited WAYLAND_DISPLAY,
connects stdin to `/dev/null`, and execs literal argv. Transport EOF/reset before
any setup-reply bytes is retryable within the same total10s readiness deadline;
partial/malformed replies and explicit authentication rejection fail immediately. `emacsclient` resolves to
the configured X11 package. Other programs resolve as an executable with no
shell interpolation. Launch target stdout/stderr are application output.

## Relay and recovery

The private internal `_relay` entry starts exactly one socat listener with
`UNIX-LISTEN:<private>/X1,fork,mode=0600,umask=077,unlink-close=0` and
`VSOCK-CONNECT:2:106001`. No TCP, automatic unlink, cookie environment or auth
polling is used. The supervisor records filesystem dev/inode/uid and verifies
the listener's separate kernel socket inode belongs to its actual child.
Systemd owns the entire cgroup. Cleanup needs unchanged socket identity, no
kernel socket using that pathname, no surviving relay cgroup children, and
matching supervisor metadata. Foreign sockets/files/symlinks are preserved.
Authority survives clean relay stop/restart; the shared directory is never a
systemd RuntimeDirectory subject to automatic mode repair or credential deletion.

A crash before metadata or cleanup can leave a socket or ownership record. The
next start fails closed rather than guessing ownership. Recovery is explicitly
manual after the controller has confirmed the owned Windows server is stopped,
no connected clients remain, and guest relay/cgroup processes are gone. Inspect
exact paths and ownership before archiving/removing that confirmed stale state;
do not use a blanket `rm -f`, and do not rotate credentials for a live server.
WSLg's `/tmp/.X11-unix` and global configuration are untouched.

Development acceptance may invoke `probe --direct-vsock --auth=stdin` with the
same canonical binary input, or `probe --direct-vsock --auth=none` with EOF.
These connect directly to CID2/module port without touching runtime credentials.
Wrong/absent cookie rejection is nonzero and never Ready. Valid direct setup
emits `result=authenticated`, distinctly from normal Unix relay `result=ready`.
The manager does not invoke these development-only flags.

## Verification and provenance

Run the complete Linux helper suite:

```console
python3 -m unittest discover -s integration/nixos -v
ruff check integration/nixos
ruff format --check integration/nixos
nixfmt --check integration/nixos/*.nix
deadnix --fail integration/nixos
statix check integration/nixos
```

`tests.nix` takes `{ pkgs, homeManager, kittymacs }` from a pinned consumer flake
and exposes `evaluation` and `vm` derivations. For the current EmptyNet pin:

```console
nix build --impure --no-link --print-build-logs --expr '
  let f = builtins.getFlake "/home/Empty/src/EmptyNet";
      pkgs = import f.inputs.nixpkgs { system = "x86_64-linux"; };
  in (import ./integration/nixos/tests.nix {
    inherit pkgs; homeManager = f.inputs.home-manager; kittymacs = f.inputs.kittymacs;
  }).vm'
```

The VM runs real systemd/socat, protected-file and stale-socket tests, then uses
an isolated Unix bridge to a real Xvfb for positive/wrong/absent-cookie exchange
and installed Xlib selector0 lookup. Its immutable server authority fixture is
separate from the client fixture, so Xvfb resets cannot adopt a wrong test cookie.
It checks absence of TCP6001. This does not establish live Hyper-V VSOCK/server,
Windows native windows/clipboard, installed kittymacs GUI or WSL idle behavior;
those require the controller's live acceptance against the built artifacts.

EmptyNet consumes reviewed tracked copies under `tools/vendor/vcxsrv-wsl/`, with
`SOURCE.json` recording full source revision and per-file SHA256. Preserve every
tracked sibling here, including VM and Python test fixtures, so tests remain
runnable. Import from authoritative `wiki/mcqueenix.org`, tangle normally, stage
new source files, evaluate and build before any coordinated activation. Do not
make a permanent `/mnt/c` or untracked absolute import. Provenance records the
reviewed source commit; copied bytes must match exactly. No flake lock update is
required for this vendoring route.

Relevant upstream behavior: [HM Emacs service](https://raw.githubusercontent.com/nix-community/home-manager/release-26.05/modules/services/emacs.nix)
preserves live sessions and uses a login shell; [socat manual](https://raw.githubusercontent.com/dest-unreach/socat1/master/doc/socat.yo)
defines pathname unlink controls. These constraints are covered by this module's
service dependencies and supervisor ownership checks.