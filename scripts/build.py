# build the mod
# usage: mcmod build

import subprocess

if __name__ == "__main__":
    subprocess.run(["powershell", "-NoProfile", "-c", "./gradlew build"])
    print("The output jar is at build/libs/")