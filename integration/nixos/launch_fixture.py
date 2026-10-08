"""Real exec boundary fixture; stdin must have been replaced with /dev/null."""

import json
import os
import sys

print(
    json.dumps(
        {
            "args": sys.argv[1:],
            "stdin": sys.stdin.read(),
            "env": {
                key: os.environ[key]
                for key in ("DISPLAY", "XAUTHORITY", "WAYLAND_DISPLAY")
                if key in os.environ
            },
        }
    )
)
