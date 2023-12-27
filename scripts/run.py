# run client or server
# usage: mcmod run <client|server>

import subprocess
import sys

if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("usage: mcmod run <client|server>")
        exit(1)

    if sys.argv[1] == "client":
        subprocess.run(["powershell", "-NoProfile", "-c", "./gradlew runClient"])
    elif sys.argv[1] == "server":
        subprocess.run(["powershell", "-NoProfile", "-c", "./gradlew runServer"])
    else:
        print("usage: mcmod run <client|server>")
        exit(1)
