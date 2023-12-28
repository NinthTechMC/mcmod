"""
Setup eclipse workspace

Usage:
    mcmod eclipse
"""

import mcmod
import os
import shutil

if __name__ == "__main__":
    mcmod.bootstrap_help()

    print("setting up eclipse workspace")

    root = mcmod.find_root()
    if os.path.exists(os.path.join(root, "eclipse")):
        print("eclipse workspace already exists!")
        if input("do you want to delete it and recreate it? (y/n) ") != "y":
            exit(1)
        shutil.rmtree(os.path.join(root, "eclipse"))
    
    if os.path.exists(os.path.join(root, ".classpath")):
        os.remove(os.path.join(root, ".classpath"))

    if os.path.exists(os.path.join(root, ".project")):
        os.remove(os.path.join(root, ".project"))
    
    if os.path.exists(os.path.join(root, ".settings")):
        shutil.rmtree(os.path.join(root, ".settings"))
    
    mcmod.run_cmd("./gradlew eclipse", cwd=root)