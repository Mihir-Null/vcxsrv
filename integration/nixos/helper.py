#!/usr/bin/env python3
"""Fixed, shell-free guest boundary. Credentials never enter diagnostics.

Nix installs this file with a pinned Python shebang and immutable sibling
etc/config.json. Internal relay supervision shares the same safe filesystem
code as bootstrap; the manager only invokes bootstrap, probe and launch.
"""

import contextlib
from dataclasses import dataclass
import fcntl
import json
import os
from pathlib import Path
import pwd
import secrets
import select
import shutil
import signal
import socket
import stat
import struct
import subprocess
import sys
import time

NAME = b"MIT-MAGIC-COOKIE-1"
ERRORS = {
    "invalid_arguments": 2,
    "invalid_record": 2,
    "account_mismatch": 3,
    "config_mismatch": 3,
    "unsafe_path": 4,
    "relay_unavailable": 5,
    "credential_missing": 6,
    "credential_conflict": 6,
    "x11_rejected": 7,
    "x11_protocol": 7,
    "deadline": 8,
}


class Failure(Exception):
    def __init__(self, code):
        self.code = code
        super().__init__(code)


def remaining(deadline):
    seconds = deadline - time.monotonic()
    if seconds <= 0:
        raise Failure("deadline")
    return seconds


@dataclass(frozen=True)
class Config:
    user: str
    display: int
    port: int
    socat: str
    systemctl: str
    emacsclient: str

    def validate(self):
        if (
            not isinstance(self.user, str)
            or not self.user
            or type(self.display) is not int
            or not 0 <= self.display <= 99
            or type(self.port) is not int
            or self.port != 106000 + self.display
            or any(
                not isinstance(path, str) or not path.startswith("/")
                for path in (self.socat, self.systemctl, self.emacsclient)
            )
        ):
            raise Failure("config_mismatch")
        try:
            account = pwd.getpwnam(self.user)
        except KeyError:
            raise Failure("account_mismatch") from None
        if account.pw_uid != os.geteuid() or os.getuid() != os.geteuid():
            raise Failure("account_mismatch")
        return account.pw_uid


def load_config():
    try:
        path = Path(__file__).resolve().parent.parent / "etc/config.json"
        config = Config(**json.loads(path.read_text()))
    except (OSError, ValueError, TypeError):
        raise Failure("config_mismatch") from None
    config.validate()
    return config


def decode_record(record, display):
    """Exactly one canonical FamilyWild record, no arbitrary length allocation."""
    if len(record) > 64 or len(record) < 10 or record[:2] != b"\xff\xff":
        raise Failure("invalid_record")
    position = 2
    fields = []
    for _ in range(4):
        if position + 2 > len(record):
            raise Failure("invalid_record")
        length = int.from_bytes(record[position : position + 2], "big")
        position += 2
        if position + length > len(record):
            raise Failure("invalid_record")
        fields.append(record[position : position + length])
        position += length
    if (
        position != len(record)
        or fields[:3] != [b"", str(display).encode("ascii"), NAME]
        or len(fields[3]) != 16
    ):
        raise Failure("invalid_record")
    return fields[3]


def encode_record(cookie, display):
    fields = (b"", str(display).encode("ascii"), NAME, cookie)
    return b"\xff\xff" + b"".join(
        struct.pack(">H", len(field)) + field for field in fields
    )


def read_input(fd, deadline):
    data = bytearray()
    while True:
        readable, _, _ = select.select([fd], [], [], remaining(deadline))
        if not readable:
            raise Failure("deadline")
        block = os.read(fd, 65 - len(data))
        if not block:
            return bytes(data)
        data.extend(block)
        if len(data) > 64:
            raise Failure("invalid_record")


def identity(st):
    return [st.st_dev, st.st_ino, st.st_uid]


def validate_file(st, uid):
    if (
        not stat.S_ISREG(st.st_mode)
        or st.st_uid != uid
        or stat.S_IMODE(st.st_mode) != 0o600
        or st.st_nlink != 1
    ):
        raise Failure("unsafe_path")


def validate_directory(st, uid):
    if (
        not stat.S_ISDIR(st.st_mode)
        or st.st_uid != uid
        or stat.S_IMODE(st.st_mode) != 0o700
    ):
        raise Failure("unsafe_path")


class Runtime:
    """Own opened trusted runtime/private dirfds; all file operations use them."""

    def __init__(self, root_fd, root_path, uid, create):
        self.root_fd = root_fd
        self.fd = None
        self.uid = uid
        self.path = root_path / "vcxsrv-wsl"
        try:
            validate_directory(os.fstat(root_fd), uid)
            if create:
                try:
                    os.mkdir("vcxsrv-wsl", 0o700, dir_fd=root_fd)
                except FileExistsError:
                    pass
            self.fd = os.open(
                "vcxsrv-wsl",
                os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC,
                dir_fd=root_fd,
            )
            validate_directory(os.fstat(self.fd), uid)
        except FileNotFoundError:
            self.close()
            raise Failure("relay_unavailable") from None
        except (OSError, Failure):
            self.close()
            raise Failure("unsafe_path") from None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def close(self):
        for fd in (self.fd, self.root_fd):
            if fd is not None:
                os.close(fd)
        self.fd = self.root_fd = None

    def stat(self, name):
        try:
            return os.stat(name, dir_fd=self.fd, follow_symlinks=False)
        except FileNotFoundError:
            return None
        except OSError:
            raise Failure("unsafe_path") from None

    def read_file(self, name, limit):
        fd = None
        try:
            fd = os.open(
                name,
                os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC,
                dir_fd=self.fd,
            )
            validate_file(os.fstat(fd), self.uid)
            data = os.read(fd, limit + 1)
            if len(data) > limit:
                raise Failure("unsafe_path")
            return data
        except FileNotFoundError:
            return None
        except OSError:
            raise Failure("unsafe_path") from None
        finally:
            if fd is not None:
                os.close(fd)

    @contextlib.contextmanager
    def lock(self, deadline):
        fd = None
        try:
            fd = os.open(
                "lock",
                os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC,
                0o600,
                dir_fd=self.fd,
            )
            validate_file(os.fstat(fd), self.uid)
            while True:
                remaining(deadline)
                try:
                    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    break
                except BlockingIOError:
                    time.sleep(min(0.02, remaining(deadline)))
            yield
        except OSError:
            raise Failure("unsafe_path") from None
        finally:
            if fd is not None:
                os.close(fd)

    def atomic_write(self, name, data):
        temp = None
        fd = None
        try:
            for _ in range(8):
                candidate = ".tmp-" + secrets.token_hex(16)
                try:
                    fd = os.open(
                        candidate,
                        os.O_WRONLY
                        | os.O_CREAT
                        | os.O_EXCL
                        | os.O_NOFOLLOW
                        | os.O_CLOEXEC,
                        0o600,
                        dir_fd=self.fd,
                    )
                    temp = candidate
                    break
                except FileExistsError:
                    continue
            if fd is None:
                raise Failure("unsafe_path")
            validate_file(os.fstat(fd), self.uid)
            view = memoryview(data)
            while view:
                written = os.write(fd, view)
                if written == 0:
                    raise Failure("unsafe_path")
                view = view[written:]
            os.fsync(fd)
            existing = self.stat(name)
            if existing is not None:
                validate_file(existing, self.uid)
            os.replace(temp, name, src_dir_fd=self.fd, dst_dir_fd=self.fd)
            temp = None
            os.fsync(self.fd)
        except OSError:
            raise Failure("unsafe_path") from None
        finally:
            if fd is not None:
                os.close(fd)
            if temp is not None:
                with contextlib.suppress(OSError):
                    os.unlink(temp, dir_fd=self.fd)

    def store_authority(self, record):
        decode_record(record, 0)
        existing = self.read_file("authority", 64)
        if existing is not None:
            decode_record(existing, 0)
            if existing != record:
                raise Failure("credential_conflict")
            return
        self.atomic_write("authority", record)

    def authority(self):
        record = self.read_file("authority", 64)
        if record is None:
            raise Failure("credential_missing")
        return decode_record(record, 0)

    def require_absent_socket(self, name):
        if self.stat(name) is not None:
            raise Failure("unsafe_path")

    def socket_identity(self, name):
        st = self.stat(name)
        if st is None:
            raise Failure("relay_unavailable")
        if (
            not stat.S_ISSOCK(st.st_mode)
            or st.st_uid != self.uid
            or stat.S_IMODE(st.st_mode) != 0o600
        ):
            raise Failure("unsafe_path")
        return identity(st)

    def validate_path_identity(self):
        """Environment/socat need a normal path: reject renamed/replaced dirs."""
        try:
            current = self.path.lstat()
        except OSError:
            raise Failure("unsafe_path") from None
        if identity(current) != identity(os.fstat(self.fd)):
            raise Failure("unsafe_path")
        validate_directory(current, self.uid)


def open_runtime(config, create=False):
    uid = config.validate()
    path = Path("/run/user") / str(uid)
    if "XDG_RUNTIME_DIR" in os.environ and os.environ["XDG_RUNTIME_DIR"] != str(path):
        raise Failure("unsafe_path")
    fd = None
    try:
        fd = os.open("/", os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC)
        for component in ("run", "user", str(uid)):
            child = os.open(
                component,
                os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC,
                dir_fd=fd,
            )
            os.close(fd)
            fd = child
            st = os.fstat(fd)
            if component != str(uid) and (st.st_uid != 0 or st.st_mode & 0o022):
                raise Failure("unsafe_path")
        result = Runtime(fd, path, uid, create)
        fd = None  # Runtime owns it, including constructor failure.
        return result
    except FileNotFoundError:
        raise Failure("unsafe_path") from None
    except OSError:
        raise Failure("unsafe_path") from None
    finally:
        if fd is not None:
            # Runtime closes on a failed constructor, so transfer before call.
            with contextlib.suppress(OSError):
                os.close(fd)


def receive_exact(stream, size, deadline):
    data = bytearray()
    while len(data) < size:
        stream.settimeout(remaining(deadline))
        block = stream.recv(size - len(data))
        if not block:
            raise Failure("x11_protocol")
        data.extend(block)
    return bytes(data)


def handshake(stream, cookie, deadline):
    name = NAME if cookie is not None else b""
    auth = cookie if cookie is not None else b""
    packet = struct.pack("<BBHHHHH", ord("l"), 0, 11, 0, len(name), len(auth), 0)
    packet += name + bytes((-len(name)) % 4) + auth + bytes((-len(auth)) % 4)
    try:
        stream.settimeout(remaining(deadline))
        stream.sendall(packet)
        head = receive_exact(stream, 8, deadline)
        status, reason_length, major, minor, units = struct.unpack("<BBHHH", head)
        if major != 11 or minor != 0 or units * 4 > 65536:
            raise Failure("x11_protocol")
        body = receive_exact(stream, units * 4, deadline)
        if status in (0, 2):
            if status == 0 and reason_length > len(body):
                raise Failure("x11_protocol")
            raise Failure("x11_rejected")
        if status != 1 or len(body) < 32:
            raise Failure("x11_protocol")
    except TimeoutError:
        raise Failure("deadline") from None
    except OSError:
        raise Failure("x11_protocol") from None


def probe(config, deadline, direct=False, cookie=None):
    stream = None
    try:
        if direct:
            stream = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
            address = (2, config.port)
            stream.settimeout(remaining(deadline))
            stream.connect(address)
            handshake(stream, cookie, deadline)
        else:
            with open_runtime(config) as runtime:
                cookie = runtime.authority()
                name = "X" + str(config.display)
                runtime.socket_identity(name)
                runtime.validate_path_identity()
                stream = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                stream.settimeout(remaining(deadline))
                # Keep dirfd pinned through the full authenticated exchange.
                stream.connect("/proc/self/fd/" + str(runtime.fd) + "/" + name)
                handshake(stream, cookie, deadline)
    except TimeoutError:
        raise Failure("deadline") from None
    except OSError:
        raise Failure("relay_unavailable") from None
    finally:
        if stream is not None:
            stream.close()


def start_relay(config, deadline):
    """No lock held: declarative service preflight needs the same lock."""
    try:
        result = subprocess.run(
            [config.systemctl, "--user", "start", "vcxsrv-wsl-relay.service"],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=remaining(deadline),
            check=False,
        )
    except subprocess.TimeoutExpired:
        raise Failure("deadline") from None
    except OSError:
        raise Failure("relay_unavailable") from None
    if result.returncode != 0:
        raise Failure("relay_unavailable")


def bootstrap(config, record):
    cookie = decode_record(record, config.display)
    deadline = time.monotonic() + 5
    with open_runtime(config, True) as runtime:
        with runtime.lock(deadline):
            runtime.store_authority(encode_record(cookie, 0))
    start_relay(config, deadline)


def exec_target(config, runtime, argv):
    if not argv or not argv[0] or any("\x00" in arg for arg in argv):
        raise Failure("invalid_arguments")
    runtime.validate_path_identity()
    program = config.emacsclient if argv[0] == "emacsclient" else argv[0]
    if not os.path.isabs(program):
        program = shutil.which(program)
    if program is None:
        raise Failure("invalid_arguments")
    env = dict(os.environ)
    env["DISPLAY"] = "unix:" + str(runtime.path / ("X" + str(config.display)))
    env["XAUTHORITY"] = str(runtime.path / "authority")
    env.pop("WAYLAND_DISPLAY", None)
    try:
        fd = os.open("/dev/null", os.O_RDONLY | os.O_CLOEXEC)
        os.dup2(fd, 0)
        os.close(fd)
        os.execve(program, [program, *argv[1:]], env)
    except OSError:
        raise Failure("invalid_arguments") from None


def launch(config, argv):
    if not argv or not argv[0]:
        raise Failure("invalid_arguments")
    deadline = time.monotonic() + 10
    while True:
        try:
            probe(config, min(deadline, time.monotonic() + 2))
            break
        except Failure as error:
            if error.code not in ("relay_unavailable", "deadline"):
                raise
            time.sleep(min(0.1, remaining(deadline)))
    with open_runtime(config) as runtime:
        exec_target(config, runtime, argv)


def unix_entries(path):
    """Kernel socket inodes differ from filesystem inodes; don't mix them."""
    try:
        lines = Path("/proc/net/unix").read_text().splitlines()[1:]
    except OSError:
        raise Failure("unsafe_path") from None
    return [
        fields
        for line in lines
        if len(fields := line.split(maxsplit=7)) == 8 and fields[7] == str(path)
    ]


def listener_owned_by(pid, path):
    entries = unix_entries(path)
    listening = {fields[6] for fields in entries if int(fields[3], 16) & 0x10000}
    try:
        for fd in Path("/proc").joinpath(str(pid), "fd").iterdir():
            target = os.readlink(fd)
            if target.startswith("socket:[") and target[8:-1] in listening:
                return True
    except OSError:
        return False
    return False


def cgroup_empty_except_self():
    try:
        groups = Path("/proc/self/cgroup").read_text().splitlines()
        group = next(line[3:] for line in groups if line.startswith("0::"))
        path = Path("/sys/fs/cgroup") / group.lstrip("/") / "cgroup.procs"
        return set(path.read_text().split()) <= {str(os.getpid())}
    except (OSError, StopIteration):
        return False


def cleanup_socket(
    runtime, name, owned_identity, children_gone=cgroup_empty_except_self
):
    st = runtime.stat(name)
    if st is None:
        return True
    if (
        not stat.S_ISSOCK(st.st_mode)
        or identity(st) != owned_identity
        or unix_entries(runtime.path / name)
        or not children_gone()
    ):
        return False
    os.unlink(name, dir_fd=runtime.fd)
    os.fsync(runtime.fd)
    return True


def relay(config):
    """One socat in systemd's cgroup; conservative cleanup after all children."""
    stopped = False

    def stop(_signal, _frame):
        nonlocal stopped
        stopped = True

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    name = "X" + str(config.display)
    owned = None
    child = None
    with open_runtime(config, True) as runtime:
        try:
            with runtime.lock(time.monotonic() + 3):
                runtime.validate_path_identity()
                runtime.require_absent_socket(name)
                if runtime.stat("socket-owner.json") is not None:
                    raise Failure("unsafe_path")
                child = subprocess.Popen(
                    [
                        config.socat,
                        "UNIX-LISTEN:"
                        + str(runtime.path / name)
                        + ",fork,mode=0600,umask=077,unlink-close=0",
                        "VSOCK-CONNECT:2:" + str(config.port),
                    ],
                    stdin=subprocess.DEVNULL,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    start_new_session=True,
                )
                deadline = time.monotonic() + 2
                while not stopped and child.poll() is None:
                    if runtime.stat(name) is not None and listener_owned_by(
                        child.pid, runtime.path / name
                    ):
                        owned = runtime.socket_identity(name)
                        runtime.atomic_write(
                            "socket-owner.json",
                            json.dumps(
                                {
                                    "version": 1,
                                    "socket": name,
                                    "identity": owned,
                                    "supervisor": os.getpid(),
                                },
                                separators=(",", ":"),
                            ).encode(),
                        )
                        break
                    time.sleep(min(0.01, remaining(deadline)))
                if owned is None:
                    raise Failure("relay_unavailable")
            while not stopped and child.poll() is None:
                time.sleep(0.1)
            if not stopped:
                raise Failure("relay_unavailable")
        except OSError:
            raise Failure("relay_unavailable") from None
        finally:
            if child is not None:
                with contextlib.suppress(ProcessLookupError):
                    os.killpg(child.pid, signal.SIGTERM)
                with contextlib.suppress(subprocess.TimeoutExpired):
                    child.wait(timeout=1)
                with contextlib.suppress(ProcessLookupError):
                    os.killpg(child.pid, signal.SIGKILL)
                with contextlib.suppress(subprocess.TimeoutExpired):
                    child.wait(timeout=1)
            if owned is not None:
                deadline = time.monotonic() + 2
                while not cgroup_empty_except_self() and time.monotonic() < deadline:
                    time.sleep(0.02)
                with runtime.lock(time.monotonic() + 0.5):
                    metadata = runtime.read_file("socket-owner.json", 1024)
                    try:
                        expected = json.loads(metadata) if metadata else None
                    except ValueError:
                        expected = None
                    if (
                        expected is not None
                        and expected.get("identity") == owned
                        and expected.get("supervisor") == os.getpid()
                        and cleanup_socket(runtime, name, owned)
                    ):
                        os.unlink("socket-owner.json", dir_fd=runtime.fd)
                        os.fsync(runtime.fd)


def emit(config, verb, result, code=None):
    data = {
        "version": 1,
        "verb": verb,
        "result": result,
        "display": config.display if config else 0,
        "port": config.port if config else 106000,
    }
    if code is not None:
        data["code"] = code
    print(json.dumps(data, separators=(",", ":")), flush=True)


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    verb = (
        argv[0]
        if argv and argv[0] in ("bootstrap", "probe", "launch", "_relay")
        else "probe"
    )
    config = None
    try:
        config = load_config()
        if argv == ["bootstrap"]:
            record = read_input(0, time.monotonic() + 2)
            bootstrap(config, record)
            emit(config, verb, "bootstrapped")
        elif argv == ["probe"]:
            if read_input(0, time.monotonic() + 2):
                raise Failure("invalid_arguments")
            probe(config, time.monotonic() + 2)
            emit(config, verb, "ready")
        elif argv in (
            ["probe", "--direct-vsock", "--auth=stdin"],
            ["probe", "--direct-vsock", "--auth=none"],
        ):
            record = read_input(0, time.monotonic() + 2)
            if argv[-1] == "--auth=none":
                if record:
                    raise Failure("invalid_arguments")
                cookie = None
            else:
                cookie = decode_record(record, config.display)
            probe(config, time.monotonic() + 2, True, cookie)
            emit(
                config, verb, "authenticated"
            )  # Development evidence, never normal Ready.
        elif len(argv) >= 3 and argv[:2] == ["launch", "--"]:
            if read_input(0, time.monotonic() + 2):
                raise Failure("invalid_arguments")
            launch(config, argv[2:])
        elif argv == ["_relay"]:
            relay(config)
        else:
            raise Failure("invalid_arguments")
        return 0
    except Failure as error:
        if verb != "_relay":
            emit(config, verb, "error", error.code)
        print("vcxsrv-wsl: " + error.code, file=sys.stderr)
        return ERRORS[error.code]
    except Exception:
        # Never expose tracebacks, arbitrary filenames, payloads or stdin.
        if verb != "_relay":
            emit(config, verb, "error", "unsafe_path")
        print("vcxsrv-wsl: unsafe_path", file=sys.stderr)
        return 4


if __name__ == "__main__":
    sys.exit(main())
