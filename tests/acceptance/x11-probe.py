#!/usr/bin/env python3
"""Development-only real X11 setup probe. Credentials never enter argv/output."""
import argparse
import socket
import struct
import sys
import time


def authority_cookie(stream, display):
    def field():
        prefix = stream.read(2)
        if len(prefix) != 2:
            raise ValueError("truncated authority file")
        size = struct.unpack("!H", prefix)[0]
        data = stream.read(size)
        if len(data) != size:
            raise ValueError("truncated authority field")
        return data

    while True:
        family = stream.read(2)
        if not family:
            raise ValueError("no matching MIT cookie in authority file")
        if len(family) != 2:
            raise ValueError("truncated authority family")
        _, number, name, data = field(), field(), field(), field()
        if number == str(display).encode("ascii") and name == b"MIT-MAGIC-COOKIE-1" and len(data) == 16:
            return data


def setup_request(cookie):
    name = b"MIT-MAGIC-COOKIE-1" if cookie else b""
    pad = lambda data: data + b"\0" * (-len(data) % 4)
    return struct.pack("<BBHHHHH", ord("l"), 0, 11, 0, len(name), len(cookie), 0) + pad(name) + pad(cookie)


def setup_result(connection, cookie, timeout):
    deadline = time.monotonic() + timeout

    def read_exact(size):
        result = bytearray()
        while len(result) < size:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("X11 setup timed out")
            connection.settimeout(remaining)
            chunk = connection.recv(size - len(result))
            if not chunk:
                raise ConnectionError("X11 setup closed before a complete response")
            result.extend(chunk)
        return bytes(result)

    connection.settimeout(timeout)
    connection.sendall(setup_request(cookie))
    status, detail, major, minor, blocks = struct.unpack("<BBHHH", read_exact(8))
    body = read_exact(blocks * 4)
    if status == 1 and major == 11 and len(body) >= 32:
        return True
    if status == 0:
        return False
    raise ValueError("unexpected X11 setup response")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    transport = parser.add_mutually_exclusive_group(required=True)
    transport.add_argument("--unix", metavar="PATH")
    transport.add_argument("--vsock-port", type=int)
    parser.add_argument("--cid", type=int, default=2)
    auth = parser.add_mutually_exclusive_group()
    auth.add_argument("--authority", metavar="FILE")
    auth.add_argument("--authority-stdin", action="store_true", help="read binary Xauthority from stdin")
    parser.add_argument("--display", default="1", help="authority record selector")
    parser.add_argument("--wrong-cookie", action="store_true", help="alter the loaded cookie in memory")
    parser.add_argument("--timeout", type=float, default=5)
    args = parser.parse_args(argv)
    if args.timeout <= 0:
        parser.error("timeout must be positive")
    if args.wrong_cookie and not (args.authority or args.authority_stdin):
        parser.error("wrong-cookie requires an authority input")
    try:
        cookie = b""
        if args.authority:
            with open(args.authority, "rb") as stream:
                cookie = authority_cookie(stream, args.display)
        elif args.authority_stdin:
            cookie = authority_cookie(sys.stdin.buffer, args.display)
        if args.wrong_cookie:
            cookie = bytes([cookie[0] ^ 1]) + cookie[1:]
        family = socket.AF_UNIX if args.unix else socket.AF_VSOCK
        endpoint = args.unix if args.unix else (args.cid, args.vsock_port)
        with socket.socket(family, socket.SOCK_STREAM) as connection:
            connection.settimeout(args.timeout)
            connection.connect(endpoint)
            accepted = setup_result(connection, cookie, args.timeout)
        print("ACCEPTED" if accepted else "REJECTED")
        return 0 if accepted else 1
    except (OSError, ValueError, AttributeError) as error:
        # Do not echo exception data: filenames/captured fields may contain secrets.
        print("PROBE_ERROR: " + type(error).__name__, file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
