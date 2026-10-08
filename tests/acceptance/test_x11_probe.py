import importlib.util
import io
from pathlib import Path
import socket
import struct
import threading
import time
import unittest

spec = importlib.util.spec_from_file_location("probe", Path(__file__).with_name("x11-probe.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class ProbeTests(unittest.TestCase):
    def test_binary_authority_and_rejected_truncation(self):
        fields = [b"", b"1", b"MIT-MAGIC-COOKIE-1", bytes(range(16))]
        record = b"\xff\xff" + b"".join(struct.pack("!H", len(item)) + item for item in fields)
        self.assertEqual(probe.authority_cookie(io.BytesIO(record), "1"), bytes(range(16)))
        with self.assertRaises(ValueError):
            probe.authority_cookie(io.BytesIO(record[:-1]), "1")

    def exchange(self, status, cookie):
        client, server = socket.socketpair()
        observed = []

        def peer():
            with server:
                request = bytearray()
                expected = len(probe.setup_request(cookie))
                while len(request) < expected:
                    request.extend(server.recv(expected - len(request)))
                observed.append(bytes(request))
                body = bytes(32) if status else b"No!!"
                response = struct.pack("<BBHHH", status, 0, 11, 0, len(body) // 4) + body
                for byte in response:
                    server.sendall(bytes([byte]))

        worker = threading.Thread(target=peer)
        worker.start()
        try:
            with client:
                self.assertEqual(probe.setup_result(client, cookie, 1), bool(status))
        finally:
            worker.join(timeout=2)
        self.assertFalse(worker.is_alive())
        self.assertEqual(observed[0], probe.setup_request(cookie))
        self.assertEqual(struct.unpack("<BBHHHHH", observed[0][:12])[4:6], (18, 16) if cookie else (0, 0))

    def test_fragmented_acceptance_with_binary_cookie(self):
        self.exchange(1, bytes(range(16)))

    def test_real_rejection_without_cookie(self):
        self.exchange(0, b"")

    def test_bounded_read(self):
        client, server = socket.socketpair()
        started = time.monotonic()
        try:
            with self.assertRaises(TimeoutError):
                probe.setup_result(client, b"", .05)
        finally:
            client.close()
            server.close()
        self.assertLess(time.monotonic() - started, .5)


if __name__ == "__main__":
    unittest.main()
