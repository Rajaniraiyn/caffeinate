import gzip
import os
from pathlib import Path
import subprocess
from time import monotonic

platform = os.environ["PLATFORM"]
root = Path("target", os.environ["TARGET"], "probe").resolve()
suffix = ".exe" if platform == "windows" else ""
prefix = ["sudo", "--"] if platform in ("glibc", "musl") else []
profiles = ("baseline", "rebuilt", "immediate-z", "immediate-s", "immediate-no-debug")

def run(profile, args, elevated=False):
    path = root / (profile + suffix)
    result = subprocess.run([*(prefix if elevated else []), str(path), *args], capture_output=True, timeout=12)
    return result.returncode, result.stdout, result.stderr

for name in profiles:
    data = (root / (name + suffix)).read_bytes()
    print(f"{platform} {name} raw={len(data)} gzip={len(gzip.compress(data, mtime=0))} bytes", flush=True)

command = ["cmd.exe", "/d", "/c", "exit 37"] if suffix else ["/bin/sh", "-c", "sleep 2; exit 37"]
cases = [
    (["--help"], 0, False), (["--not-a-flag"], 2, False),
    (["--timeout", "NaN"], 2, False), (["--wait", "0"], 2, False),
    (["-it1"], 0, True), (["-it1", *command], 37, True),
]
if not prefix:
    cases += [(["-dt1"], 0, True), (["-ut1"], 0, True), (["-st1"], 0, True)]
for args, code, elevated in cases:
    expected = run("baseline", args, elevated)
    assert expected[0] == code, (args, expected)
    for name in profiles[1:]:
        actual = run(name, args, elevated)
        assert actual == expected, (name, args, actual, expected)

if not suffix:
    for name in profiles:
        with subprocess.Popen(["sleep", "2"]) as child:
            start = monotonic()
            result = run(name, ["-iw", str(child.pid)], True)
            elapsed = monotonic() - start
            assert result[0] == 0 and 1.5 <= elapsed <= 8, (name, result, elapsed)
print("All variants passed CLI behavior checks")
