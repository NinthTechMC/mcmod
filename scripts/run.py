"""
Run client or server.

Usage:
    mcmod run [SIDE]

Args:
    SIDE   "client" or "server". Default is client.
"""

import mcmod
import sys
import os

def agree_to_eula(root):
    eula_path = os.path.join(root, "run/eula.txt")
    if os.path.exists(eula_path):
        with open(eula_path, "r", encoding="utf-8") as f:
            for line in f:
                if line.strip() == "eula=true":
                    return
    print("Automatically agreeing to EULA to run the server.")
    print("Please read the EULA at https://account.mojang.com/documents/minecraft_eula")
    with open(eula_path, "w", encoding="utf-8") as f:
        f.write("eula=true\n")

if __name__ == "__main__":
    mcmod.bootstrap_help()
    if len(sys.argv) != 2 and len(sys.argv) != 1:
        mcmod.print_help()
        exit(1)

    root = mcmod.find_root()

    if len(sys.argv) == 1 or sys.argv[1] == "client":
        mcmod.run_cmd("./gradlew runClient", cwd=root)
    elif len(sys.argv) == 2 and sys.argv[1] == "server":
        agree_to_eula(root)
        mcmod.run_cmd("./gradlew runServer", cwd=root)
    else:
        mcmod.print_help()
        exit(1)
