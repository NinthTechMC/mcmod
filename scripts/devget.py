"""
Download dev (deobfuscated) JAR from Pistonite CDN

Usage:
    mcmod devget [NAME]

Args:
    NAME   name of the JAR file
"""

import mcmod
import requests
import sys
import os

if __name__ == "__main__":
    mcmod.bootstrap_help()
    if len(sys.argv) != 2:
        mcmod.print_help()
        exit(1)
    
    name = sys.argv[1]
    url = f"https://cdn.pistonite.org/minecraft/devjars/{name}"

    print(f"downloading {url}")
    response = requests.get(url)
    if not response.ok:
        print(f"failed to download {name}")
        exit(1)
    
    root = mcmod.find_root()
    libs_dir = os.path.join(root, "libs")
    if not os.path.exists(libs_dir):
        os.makedirs(libs_dir)
    
    with open(os.path.join(libs_dir, name), "wb") as f:
        f.write(response.content)