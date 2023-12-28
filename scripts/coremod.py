"""
Enable/remove the coremod declaration in build.gradle

Usage:
    mcmod coremod enable CLASSNAME
    mcmod coremod remove

Args:
    CLASSNAME  the short name of the coremod class (without the package)
               the full name should be com.piston.mc.<modid>.coremod.<CLASSNAME>
"""

import mcmod
import sys

def enable(classname):
    root = mcmod.find_root()
    name = mcmod.read_mcmod_info(root)["name"]
    modid = mcmod.modid_from_name(name)
    coremod_group = mcmod.coremod_group_from_modid(modid)
    coremod_class = coremod_group + "." + classname

    mcmod.write_build_gradle(
        root,
        None,
        None,
        None,
        coremod_class
    )

def disable():
    root = mcmod.find_root()
    mcmod.write_build_gradle(
        root,
        None,
        None,
        None,
        None
    )

if __name__ == "__main__":
    mcmod.bootstrap_help()
    if len(sys.argv) < 2:
        mcmod.print_help()
        exit(1)
    
    command = sys.argv[1]
    if command == "enable":
        if len(sys.argv) != 3:
            print("missing class name")
            print("try `mcmod coremod help`")
            exit(1)
        enable(sys.argv[2])
    elif command == "remove":
        disable()
    else:
        mcmod.print_help()
        exit(1)