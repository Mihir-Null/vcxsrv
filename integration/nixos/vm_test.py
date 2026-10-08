"""Runs as Empty inside an isolated NixOS VM, never the production guest."""

import ctypes
import importlib.util
import json
import os
from pathlib import Path
import socket
import signal
import subprocess
import time

ROOT = Path("/run/user/1000/vcxsrv-wsl")
HELPER = "/run/current-system/sw/bin/vcxsrv-wsl"
GOLDEN = (
    bytes.fromhex("ffff00000001310012")
    + b"MIT-MAGIC-COOKIE-1\x00\x10"
    + bytes(range(16))
)
GUEST = (
    bytes.fromhex("ffff00000001300012")
    + b"MIT-MAGIC-COOKIE-1\x00\x10"
    + bytes(range(16))
)


def call(args, stdin=b"", status=0, code=None):
    result = subprocess.run(
        [HELPER, *args], input=stdin, capture_output=True, timeout=12
    )
    assert result.returncode == status, (
        args[0],
        result.returncode,
        result.stderr.decode(),
    )
    # No fixture serialization or raw cookie may escape into protocol/logs.
    assert bytes(range(16)) not in result.stdout + result.stderr
    assert len(result.stdout) <= 1024 and result.stdout.count(b"\n") == 1
    data = json.loads(result.stdout)
    assert data["version"] == 1 and data["display"] == 1 and data["port"] == 106001
    if code:
        assert data["result"] == "error" and data["code"] == code
    return data


def ctl(*args):
    return (
        subprocess.run(["systemctl", "--user", *args], capture_output=True, check=True)
        .stdout.decode()
        .strip()
    )


def wait(condition):
    deadline = time.monotonic() + 5
    while not condition():
        assert time.monotonic() < deadline, "isolated test deadline"
        time.sleep(0.05)


def stop():
    ctl("stop", "vcxsrv-wsl-relay.service")
    wait(lambda: not (ROOT / "X1").exists())
    assert not (ROOT / "socket-owner.json").exists()


assert call(["bootstrap"], GOLDEN)["result"] == "bootstrapped"
wait(lambda: (ROOT / "X1").exists() and (ROOT / "socket-owner.json").exists())
assert ROOT.stat().st_mode & 0o777 == 0o700
assert (ROOT / "X1").stat().st_uid == os.getuid()
assert (ROOT / "X1").stat().st_mode & 0o777 == 0o600
assert (ROOT / "authority").stat().st_mode & 0o777 == 0o600
assert (ROOT / "authority").read_bytes() == GUEST
pid = ctl("show", "vcxsrv-wsl-relay.service", "--property=MainPID", "--value")
inode = (ROOT / "authority").stat().st_ino
assert call(["bootstrap"], GOLDEN)["result"] == "bootstrapped"
assert ctl("show", "vcxsrv-wsl-relay.service", "--property=MainPID", "--value") == pid
assert (ROOT / "authority").stat().st_ino == inode
call(["bootstrap"], GOLDEN[:-1] + b"x", 6, "credential_conflict")
assert (ROOT / "authority").read_bytes() == GUEST
stop()
assert (ROOT / "authority").read_bytes() == GUEST
call(["bootstrap"], GOLDEN)
wait(lambda: (ROOT / "socket-owner.json").exists())
assert (ROOT / "authority").stat().st_ino == inode
stop()
print(
    "PASS one relay, owned0600 socket, selector0 authority, duplicate/conflict, restart preservation and safe cleanup"
)

foreign = socket.socket(socket.AF_UNIX)
foreign.bind(str(ROOT / "X1"))
foreign.close()
(ROOT / "X1").chmod(0o600)
foreign_inode = (ROOT / "X1").stat().st_ino
subprocess.run(
    ["systemctl", "--user", "start", "vcxsrv-wsl-relay.service"],
    check=False,
    capture_output=True,
)
time.sleep(0.3)
assert (ROOT / "X1").stat().st_ino == foreign_inode
ctl("stop", "vcxsrv-wsl-relay.service")
assert (ROOT / "X1").stat().st_ino == foreign_inode
(ROOT / "X1").unlink()  # Test-owned fixture, never an unknown product socket.
ctl("reset-failed", "vcxsrv-wsl-relay.service")
(ROOT / "authority").rename(ROOT / "authority.saved")
(ROOT / "authority").symlink_to(ROOT / "authority.saved")
call(["bootstrap"], GOLDEN, 4, "unsafe_path")
assert (ROOT / "authority").is_symlink()
assert (ROOT / "authority.saved").read_bytes() == GUEST
(ROOT / "authority").unlink()
(ROOT / "authority.saved").rename(ROOT / "authority")
call(["probe"], status=5, code="relay_unavailable")
assert not (ROOT / "X1").exists()
assert (ROOT / "authority").stat().st_ino == inode
print(
    "PASS stale foreign socket preserved, symlink credential rejected, probe read-only"
)

# Real X server used solely inside the VM. Bridge its Unix socket to the
# private pathname, then exercise both the helper
# and installed Xlib authority lookup rather than merely decoding bytes.
# Keep server fixture immutable: Xvfb may reload its authority on reset.
server_authority = ROOT / "xvfb-test-authority"
server_authority.write_bytes(GUEST)
server_authority.chmod(0o600)
xvfb = subprocess.Popen(
    [
        "Xvfb",
        ":1",
        "-screen",
        "0",
        "640x480x24",
        "-nolisten",
        "tcp",
        "-auth",
        str(server_authority),
    ],
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
)
bridge = None
try:
    wait(lambda: Path("/tmp/.X11-unix/X1").exists())
    bridge = subprocess.Popen(
        [
            "socat",
            "UNIX-LISTEN:" + str(ROOT / "X1") + ",fork,mode=0600,unlink-close=0",
            "UNIX-CONNECT:/tmp/.X11-unix/X1",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )
    wait(lambda: (ROOT / "X1").exists())
    assert call(["probe"])["result"] == "ready"
    os.environ["DISPLAY"] = "unix:" + str(ROOT / "X1")
    os.environ["XAUTHORITY"] = str(ROOT / "authority")
    lib = ctypes.CDLL(Path("/etc/vcxsrv-wsl-tests/libX11").read_text().strip())
    lib.XOpenDisplay.argtypes = [ctypes.c_char_p]
    lib.XOpenDisplay.restype = ctypes.c_void_p
    lib.XCloseDisplay.argtypes = [ctypes.c_void_p]
    display = lib.XOpenDisplay(None)
    assert display, "installed Xlib must select guest record0 for pathname display"
    lib.XCloseDisplay(display)
    # A known fixture cookie mismatch is an actual rejection by Xvfb.
    (ROOT / "authority").write_bytes(GUEST[:-1] + b"x")
    call(["probe"], status=7, code="x11_rejected")
    (ROOT / "authority").write_bytes(GUEST)
    import sys

    spec = importlib.util.spec_from_file_location(
        "helper", "/etc/vcxsrv-wsl-tests/helper.py"
    )
    module = importlib.util.module_from_spec(spec)
    sys.modules["helper"] = module
    spec.loader.exec_module(module)
    with socket.socket(socket.AF_UNIX) as client:
        client.connect(str(ROOT / "X1"))
        try:
            module.handshake(client, None, time.monotonic() + 2)
            raise AssertionError("no-cookie X11 request unexpectedly accepted")
        except module.Failure as error:
            assert error.code == "x11_rejected"
    print(
        "PASS real Xvfb valid/wrong/absent cookie, authenticated helper probe and installed Xlib selector0 lookup"
    )
finally:
    if bridge is not None:
        os.killpg(bridge.pid, signal.SIGTERM)
        bridge.wait(timeout=5)
    xvfb.terminate()
    xvfb.wait(timeout=5)
    if (ROOT / "X1").exists():
        (ROOT / "X1").unlink()  # Known bridge test fixture after processes exit.
print("PASS isolated guest suite")
