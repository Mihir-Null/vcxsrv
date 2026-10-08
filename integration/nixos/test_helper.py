"""Contract tests: real records, Linux files/locks, sockets, subprocesses.

Run on Linux with python3 -m unittest discover -s integration/nixos -v.
Fixtures below are public test bytes, never a running server's credentials.
"""

import contextlib
import importlib.util
import json
import os
from pathlib import Path
import socket
import tempfile
import threading
import time
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("helper", HERE / "helper.py")
helper = None
if (HERE / "helper.py").exists():
    import sys

    helper = importlib.util.module_from_spec(spec)
    sys.modules["helper"] = helper
    spec.loader.exec_module(helper)

GOLDEN = (
    bytes.fromhex("ffff00000001310012")
    + b"MIT-MAGIC-COOKIE-1"
    + b"\x00\x10"
    + bytes(range(16))
)
GUEST = (
    bytes.fromhex("ffff00000001300012")
    + b"MIT-MAGIC-COOKIE-1"
    + b"\x00\x10"
    + bytes(range(16))
)


class ContractTests(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(helper, "fixed guest helper has not been implemented")
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.root.chmod(0o700)
        self.addCleanup(self.temp.cleanup)

    def runtime(self, create=True):
        fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        return helper.Runtime(fd, self.root, os.getuid(), create)

    def assert_code(self, code, operation):
        with self.assertRaises(helper.Failure) as caught:
            operation()
        self.assertEqual(caught.exception.code, code)

    def test_golden_bytes_match_rust_and_rewrite_only_guest_selector(self):
        cookie = helper.decode_record(GOLDEN, 1)
        self.assertEqual(cookie, bytes(range(16)))
        self.assertEqual(helper.encode_record(cookie, 0), GUEST)
        two_digits = (
            bytes.fromhex("ffff0000000239390012")
            + b"MIT-MAGIC-COOKIE-1\x00\x10"
            + bytes(range(16))
        )
        self.assertEqual(helper.decode_record(two_digits, 99), cookie)

    def test_all_truncations_trailing_records_wrong_display_and_huge_lengths_fail(self):
        invalid = [GOLDEN[:i] for i in range(45)]
        invalid += [
            GOLDEN + b"x",
            GOLDEN * 2,
            b"\xff\xff\xff\xff" + GOLDEN[4:],
            GOLDEN[:6] + b"2" + GOLDEN[7:],
        ]
        for record in invalid:
            with self.subTest(length=len(record)):
                self.assert_code(
                    "invalid_record", lambda: helper.decode_record(record, 1)
                )

    def test_input_requires_eof_and_bounds_bytes_and_deadline(self):
        read_fd, write_fd = os.pipe()
        self.addCleanup(os.close, read_fd)
        self.addCleanup(os.close, write_fd)
        os.write(write_fd, GOLDEN)
        self.assert_code(
            "deadline", lambda: helper.read_input(read_fd, time.monotonic() + 0.03)
        )
        os.write(write_fd, b"x" * 100)
        self.assert_code(
            "invalid_record", lambda: helper.read_input(read_fd, time.monotonic() + 1)
        )

    def test_config_rejects_display_port_and_account_mismatch(self):
        user = __import__("pwd").getpwuid(os.getuid()).pw_name
        helper.Config(
            user, 1, 106001, "/bin/false", "/bin/false", "/bin/false"
        ).validate()
        for display, port in [(100, 106100), (-1, 105999), (1, 106000)]:
            self.assert_code(
                "config_mismatch",
                lambda: helper.Config(
                    user, display, port, "/bin/false", "/bin/false", "/bin/false"
                ).validate(),
            )
        self.assert_code(
            "account_mismatch",
            lambda: helper.Config(
                "does-not-exist-vcxsrv-test",
                1,
                106001,
                "/bin/false",
                "/bin/false",
                "/bin/false",
            ).validate(),
        )

    def test_private_directory_symlink_mode_and_wrong_owner_fail_closed(self):
        target = self.root / "target"
        target.mkdir(mode=0o700)
        (self.root / "vcxsrv-wsl").symlink_to(target)
        self.assert_code("unsafe_path", self.runtime)
        (self.root / "vcxsrv-wsl").unlink()
        (self.root / "vcxsrv-wsl").mkdir(mode=0o755)
        self.assert_code("unsafe_path", self.runtime)
        (self.root / "vcxsrv-wsl").chmod(0o700)
        fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY)
        self.assert_code(
            "unsafe_path", lambda: helper.Runtime(fd, self.root, os.getuid() + 1, True)
        )

    def test_authority_is_0600_atomic_idempotent_and_conflict_preserves_original(self):
        with self.runtime() as runtime:
            with runtime.lock(time.monotonic() + 1):
                runtime.store_authority(GUEST)
                inode = (runtime.path / "authority").stat().st_ino
                runtime.store_authority(GUEST)
                self.assertEqual((runtime.path / "authority").stat().st_ino, inode)
                other = GUEST[:-1] + b"x"
                self.assert_code(
                    "credential_conflict", lambda: runtime.store_authority(other)
                )
            self.assertEqual((runtime.path / "authority").read_bytes(), GUEST)
            self.assertEqual((runtime.path / "authority").stat().st_mode & 0o777, 0o600)
            self.assertEqual(runtime.authority(), bytes(range(16)))

    def test_authority_symlink_hardlink_bad_permissions_and_nonregular_fail(self):
        with self.runtime() as runtime:
            authority = runtime.path / "authority"
            outside = self.root / "outside"
            outside.write_bytes(GUEST)
            outside.chmod(0o600)
            authority.symlink_to(outside)
            self.assert_code("unsafe_path", runtime.authority)
            authority.unlink()
            os.link(outside, authority)
            self.assert_code("unsafe_path", runtime.authority)
            authority.unlink()
            authority.write_bytes(GUEST)
            authority.chmod(0o644)
            self.assert_code("unsafe_path", runtime.authority)
            authority.unlink()
            os.mkfifo(authority, 0o600)
            self.assert_code("unsafe_path", runtime.authority)
            self.assertEqual(outside.read_bytes(), GUEST)

    def test_lock_symlink_and_hardlink_fail_without_touching_target(self):
        with self.runtime() as runtime:
            outside = self.root / "outside"
            outside.write_bytes(b"protected")
            outside.chmod(0o600)
            lock = runtime.path / "lock"
            lock.symlink_to(outside)
            self.assert_code(
                "unsafe_path", lambda: runtime.lock(time.monotonic() + 1).__enter__()
            )
            lock.unlink()
            os.link(outside, lock)
            self.assert_code(
                "unsafe_path", lambda: runtime.lock(time.monotonic() + 1).__enter__()
            )
            self.assertEqual(outside.read_bytes(), b"protected")

    def test_failed_atomic_write_leaves_no_authority_or_temp_files(self):
        with self.runtime() as runtime:
            with mock.patch.object(helper.os, "fsync", side_effect=OSError):
                self.assert_code("unsafe_path", lambda: runtime.store_authority(GUEST))
            self.assertFalse((runtime.path / "authority").exists())
            self.assertEqual(list(runtime.path.iterdir()), [])

    def test_precreated_temp_symlink_is_never_followed(self):
        with self.runtime() as runtime:
            outside = self.root / "outside"
            outside.write_bytes(b"protected")
            (runtime.path / ".tmp-fixed").symlink_to(outside)
            with mock.patch.object(helper.secrets, "token_hex", return_value="fixed"):
                self.assert_code("unsafe_path", lambda: runtime.store_authority(GUEST))
            self.assertEqual(outside.read_bytes(), b"protected")
            self.assertFalse((runtime.path / "authority").exists())

    def test_concurrent_authority_updates_use_same_lock_without_rotation(self):
        errors = []

        def bootstrap():
            try:
                with self.runtime() as runtime, runtime.lock(time.monotonic() + 2):
                    runtime.store_authority(GUEST)
            except Exception as error:
                errors.append(type(error).__name__)

        threads = [threading.Thread(target=bootstrap) for _ in range(8)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        self.assertEqual(errors, [])
        self.assertEqual((self.root / "vcxsrv-wsl/authority").read_bytes(), GUEST)

    def test_readonly_missing_runtime_and_authority_do_not_create_files(self):
        self.assert_code("relay_unavailable", lambda: self.runtime(False))
        self.assertFalse((self.root / "vcxsrv-wsl").exists())
        with self.runtime() as runtime:
            self.assert_code("credential_missing", runtime.authority)
            self.assertEqual(list(runtime.path.iterdir()), [])

    def test_foreign_socket_file_and_symlink_are_never_unlinked(self):
        with self.runtime() as runtime:
            path = runtime.path / "X1"
            path.write_bytes(b"foreign")
            self.assert_code("unsafe_path", lambda: runtime.require_absent_socket("X1"))
            self.assertEqual(path.read_bytes(), b"foreign")
            path.unlink()
            with socket.socket(socket.AF_UNIX) as server:
                server.bind(str(path))
                path.chmod(0o600)
                identity = runtime.socket_identity("X1")
            self.assert_code("unsafe_path", lambda: runtime.require_absent_socket("X1"))
            self.assertEqual(runtime.socket_identity("X1"), identity)
            path.unlink()
            path.symlink_to(self.root / "absent")
            self.assert_code("unsafe_path", lambda: runtime.require_absent_socket("X1"))
            self.assertTrue(path.is_symlink())

    def test_cleanup_requires_same_identity_no_listener_and_empty_cgroup(self):
        with self.runtime() as runtime:
            path = runtime.path / "X1"
            with socket.socket(socket.AF_UNIX) as server:
                server.bind(str(path))
                path.chmod(0o600)
                server.listen(1)
                identity = runtime.socket_identity("X1")
                self.assertFalse(
                    helper.cleanup_socket(runtime, "X1", identity, lambda: True)
                )
            self.assertFalse(
                helper.cleanup_socket(runtime, "X1", identity, lambda: False)
            )
            self.assertFalse(
                helper.cleanup_socket(runtime, "X1", [0, 0, 0], lambda: True)
            )
            self.assertTrue(
                helper.cleanup_socket(runtime, "X1", identity, lambda: True)
            )
            self.assertFalse(path.exists())

    def setup_socket(self, reply, pause=0):
        server, client = socket.socketpair()
        self.addCleanup(server.close)
        self.addCleanup(client.close)
        captured = []

        def exchange():
            captured.append(server.recv(4096))
            time.sleep(pause)
            with contextlib.suppress(OSError):
                server.sendall(reply)

        worker = threading.Thread(target=exchange)
        worker.start()
        self.addCleanup(worker.join)
        return client, captured

    def test_handshake_sends_cookie_and_accepts_complete_x11_setup(self):
        reply = b"\x01\x00\x0b\x00\x00\x00\x08\x00" + bytes(32)
        client, captured = self.setup_socket(reply)
        helper.handshake(client, bytes(range(16)), time.monotonic() + 1)
        self.assertEqual(
            captured[0],
            b"l\x00\x0b\x00\x00\x00\x12\x00\x10\x00\x00\x00MIT-MAGIC-COOKIE-1\x00\x00"
            + bytes(range(16)),
        )

    def test_x11_rejection_never_reflects_server_reason(self):
        client, _ = self.setup_socket(b"\x00\x04\x0b\x00\x00\x00\x01\x00LEAK")
        self.assert_code(
            "x11_rejected", lambda: helper.handshake(client, None, time.monotonic() + 1)
        )

    def test_x11_oversized_and_short_success_are_protocol_failures(self):
        for reply in [
            b"\x01\x00\x0b\x00\x00\x00\xff\xff",
            b"\x01\x00\x0b\x00\x00\x00\x00\x00",
        ]:
            client, _ = self.setup_socket(reply)
            self.assert_code(
                "x11_protocol",
                lambda: helper.handshake(client, None, time.monotonic() + 1),
            )

    def test_handshake_deadline_is_bounded(self):
        client, _ = self.setup_socket(b"", pause=0.08)
        self.assert_code(
            "deadline", lambda: helper.handshake(client, None, time.monotonic() + 0.02)
        )

    def test_launch_argv_environment_and_stdin_survive_real_exec(self):
        import subprocess

        fixture = HERE / "launch_fixture.py"
        config = helper.Config(
            __import__("pwd").getpwuid(os.getuid()).pw_name,
            1,
            106001,
            "/bin/false",
            "/bin/false",
            "/bin/false",
        )
        with self.runtime() as runtime:
            program = (
                "import helper; import pathlib; c=helper.Config(*"
                + repr(tuple(config.__dict__.values()))
                + "); r=helper.Runtime(__import__('os').open("
                + repr(str(self.root))
                + ",0|__import__('os').O_DIRECTORY),pathlib.Path("
                + repr(str(self.root))
                + "),__import__('os').getuid(),False); helper.exec_target(c,r,[__import__('sys').executable,"
                + repr(str(fixture))
                + ", 'snow 雪', 'x;$(touch no)', 'a\"b'])"
            )
            env = dict(os.environ, PYTHONPATH=str(HERE), WAYLAND_DISPLAY="wayland-0")
            result = subprocess.run(
                [__import__("sys").executable, "-c", program],
                input=b"secret stdin",
                capture_output=True,
                env=env,
                check=True,
            )
            data = json.loads(result.stdout)
            self.assertEqual(data["args"], ["snow 雪", "x;$(touch no)", 'a"b'])
            self.assertEqual(data["stdin"], "")
            self.assertNotIn("WAYLAND_DISPLAY", data["env"])
            self.assertEqual(data["env"]["DISPLAY"], "unix:" + str(runtime.path / "X1"))
            self.assertEqual(data["env"]["XAUTHORITY"], str(runtime.path / "authority"))

    def test_bootstrap_releases_lock_before_real_service_command(self):
        import sys

        user = __import__("pwd").getpwuid(os.getuid()).pw_name
        command = self.root / "service-command"
        command.write_text(
            "#!"
            + sys.executable
            + "\nimport fcntl,os,sys\nassert sys.argv[1:]==['--user','start','vcxsrv-wsl-relay.service']\nf=os.open("
            + repr(str(self.root / "vcxsrv-wsl/lock"))
            + ",os.O_RDWR)\nfcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)\nopen("
            + repr(str(self.root / "started"))
            + ",'w').write('started')\n"
        )
        command.chmod(0o700)
        config = helper.Config(
            user, 1, 106001, "/bin/false", str(command), "/bin/false"
        )
        with mock.patch.object(
            helper,
            "open_runtime",
            side_effect=lambda _config, create=False: self.runtime(create),
        ):
            helper.bootstrap(config, GOLDEN)
            helper.bootstrap(config, GOLDEN)
        self.assertTrue((self.root / "started").exists())
        self.assertEqual((self.root / "vcxsrv-wsl/authority").read_bytes(), GUEST)

    def test_bootstrap_invalid_record_never_creates_runtime(self):
        config = helper.Config(
            "unused", 1, 106001, "/bin/false", "/bin/false", "/bin/false"
        )
        self.assert_code(
            "invalid_record", lambda: helper.bootstrap(config, GOLDEN[:-1])
        )
        self.assertFalse((self.root / "vcxsrv-wsl").exists())

    def test_lock_contention_has_deadline_and_does_not_modify_authority(self):
        with self.runtime() as runtime, runtime.lock(time.monotonic() + 1):
            runtime.store_authority(GUEST)
            with self.runtime() as second:
                self.assert_code(
                    "deadline", lambda: second.lock(time.monotonic() + 0.03).__enter__()
                )
            self.assertEqual((runtime.path / "authority").read_bytes(), GUEST)

    def test_atomic_update_failure_preserves_existing_safe_metadata(self):
        with self.runtime() as runtime:
            runtime.atomic_write("metadata", b"old")
            with mock.patch.object(helper.os, "fsync", side_effect=OSError):
                self.assert_code(
                    "unsafe_path", lambda: runtime.atomic_write("metadata", b"new")
                )
            self.assertEqual((runtime.path / "metadata").read_bytes(), b"old")
            self.assertEqual(
                [file.name for file in runtime.path.iterdir()], ["metadata"]
            )

    def test_mismatched_runtime_environment_rejected_before_file_access(self):
        config = helper.Config(
            __import__("pwd").getpwuid(os.getuid()).pw_name,
            1,
            106001,
            "/bin/false",
            "/bin/false",
            "/bin/false",
        )
        with mock.patch.dict(os.environ, {"XDG_RUNTIME_DIR": str(self.root)}):
            self.assert_code("unsafe_path", lambda: helper.open_runtime(config, True))
        self.assertFalse((self.root / "vcxsrv-wsl").exists())

    def test_json_error_protocol_uses_fixed_schema_without_payload(self):
        import io

        config = helper.Config(
            "unused", 1, 106001, "/bin/false", "/bin/false", "/bin/false"
        )
        for code, status in [
            ("credential_conflict", 6),
            ("deadline", 8),
            ("x11_rejected", 7),
        ]:
            stdout, stderr = io.StringIO(), io.StringIO()
            with (
                mock.patch.object(helper, "load_config", return_value=config),
                mock.patch.object(helper, "read_input", return_value=b""),
                mock.patch.object(helper, "probe", side_effect=helper.Failure(code)),
                contextlib.redirect_stdout(stdout),
                contextlib.redirect_stderr(stderr),
            ):
                self.assertEqual(helper.main(["probe"]), status)
            self.assertEqual(
                json.loads(stdout.getvalue()),
                {
                    "version": 1,
                    "verb": "probe",
                    "result": "error",
                    "display": 1,
                    "port": 106001,
                    "code": code,
                },
            )
            self.assertEqual(stdout.getvalue().count("\n"), 1)
            self.assertLess(len(stdout.getvalue().encode()), 1024)
            self.assertNotIn("Traceback", stderr.getvalue())

    def test_unexpected_exception_is_sanitized_and_nonzero(self):
        import io

        stdout, stderr = io.StringIO(), io.StringIO()
        with (
            mock.patch.object(
                helper, "load_config", side_effect=RuntimeError("SECRET PAYLOAD")
            ),
            contextlib.redirect_stdout(stdout),
            contextlib.redirect_stderr(stderr),
        ):
            self.assertEqual(helper.main(["probe"]), 4)
        self.assertNotIn("SECRET", stdout.getvalue() + stderr.getvalue())
        self.assertEqual(json.loads(stdout.getvalue())["code"], "unsafe_path")


if __name__ == "__main__":
    unittest.main()
