"""Shared utilities"""
import subprocess
import sys
import os
import json

# argv[0] is the script path, use it to get the script root directory
SCRIPT_PATH = ""
SCRIPT_ROOT = ""
VERBOSE = False

def run_cmd(cmd, cwd=None):
    if VERBOSE:
        print(f"[verbose] {cmd}")
    subprocess.run(["powershell", "-NoProfile", "-c", cmd], cwd=cwd)

def bootstrap():
    global SCRIPT_PATH
    global SCRIPT_ROOT
    global VERBOSE
    SCRIPT_PATH = sys.argv[0]
    SCRIPT_ROOT = os.path.dirname(SCRIPT_PATH)
    if len(sys.argv) > 1 and sys.argv[1] == "-V":
        VERBOSE = True
        sys.argv = [sys.argv[0]] + sys.argv[2:]
        print(f"[verbose] script path is {SCRIPT_PATH}")
        print(f"[verbose] args: {sys.argv[1:]}")
    else:
        VERBOSE = False

def bootstrap_help():
    bootstrap()
    if len(sys.argv) > 1 and sys.argv[1] in ["help", "--help", "-h", "-help", "-?", "?"]:
        print_help()
        exit(0)

def print_help():
    with open(SCRIPT_PATH, "r", encoding="utf-8") as f:
        reading = False
        for line in f:
            if line.strip() == '"""':
                if reading:
                    break
                else:
                    reading = True
                continue
            if reading:
                print(line.rstrip())

def print_subcommands():
    name_descs = []
    for name in os.listdir(SCRIPT_ROOT):
        if name.endswith(".py"):
            script_path = os.path.join(SCRIPT_ROOT, name)
            script_desc = ""
            with open(script_path, "r", encoding="utf-8") as f:
                reading = False
                for line in f:
                    if line.strip() == '"""':
                        reading = True
                        continue
                    if reading:
                        script_desc = line.strip()
                        break
            name_descs.append((name[:-3], script_desc))
    
    for name, script_desc in sorted(name_descs, key=lambda x: x[0]):
        print(f"    {name:12}  {script_desc}")

def print_version():
    print("mcmod ", end="")
    sys.stdout.flush()
    run_cmd(f"git -C {SCRIPT_ROOT} rev-parse HEAD")

def find_root(cur_path = "."):
    cur_path = os.path.abspath(cur_path)
    while not os.path.exists(os.path.join(cur_path, "build.gradle")):
        cur_path = os.path.dirname(cur_path)
        if not cur_path:
            print("could not detect root directory of mod.")
            print("make sure you are in a mod repo.")
            exit(1)
    if VERBOSE:
        print(f"[verbose] mod root is {cur_path}")
    return cur_path

def read_mcmod_info(root):
    with open(os.path.join(root, "src/main/resources/mcmod.info"), "r", encoding="utf-8") as f:
        info = json.load(f)[0]
    return info

def write_mcmod_info(root, info):
    with open(os.path.join(root, "src/main/resources/mcmod.info"), "w", encoding="utf-8") as f:
        json.dump([info], f, indent=4)

def read_version(root):
    with open(os.path.join(root, "build.gradle"), "r", encoding="utf-8") as f:
        for line in f:
            if line.startswith("version"):
                version = line.split("=")[1].strip().strip("'")
                return version
    return None

def read_coremod_class(root):
    with open(os.path.join(root, "build.gradle"), "r", encoding="utf-8") as f:
        has_coremod = False
        coremod = None
        for line in f:
            if line.startswith("// coremod"):
                if has_coremod:
                    break
                has_coremod = True
                continue
            if has_coremod:
                line = line.strip()
                if line.startswith("attributes 'FMLCorePlugin'"):
                    coremod = line.split(":")[1].strip().strip("'")
                    break
        if has_coremod and coremod:
            return coremod
    return None

def write_build_gradle(root, version, group, archive_base_name, coremod_class):
    if coremod_class:
        coremod_section =f"""// coremod
jar {{
    manifest {{
        attributes 'FMLCorePlugin': '{coremod_class}'
        attributes 'FMLCorePluginContainsFMLMod': 'true'
    }}
}}
// coremod
"""
    else:
        coremod_section = ""
    with open(os.path.join(root, "build.gradle"), "r", encoding="utf-8") as f:
        file = [x for x in f]
    with open(os.path.join(root, "build.gradle"), "w", encoding="utf-8") as f:
        in_coremod = False
        for line in file:
            if version and line.startswith("version"):
                line = f"version = '{version}'\n"
            elif group and line.startswith("group"):
                line = f"group = '{group}'\n"
            elif archive_base_name and line.startswith("archivesBaseName"):
                line = f"archivesBaseName = '{archive_base_name}'\n"
            elif line.startswith("// coremod"):
                in_coremod = not in_coremod
                continue
            elif line.startswith("dependencies {"):
                f.write(coremod_section)
            if not in_coremod:
                f.write(line)

def modid_from_name(name):
    return name.lower().replace(" ", "")

def project_name_from_name(name):
    return name.replace(" ", "")

def archive_base_name_from_name(name):
    return name.lower().replace(" ", "-")

def group_from_modid(modid):
    return "com.piston.mc." + modid

def coremod_group_from_modid(modid):
    return "com.piston.mc." + modid + ".coremod"

def source_root_from_modid(modid):
    return "src/main/java/com/piston/mc/" + modid

def asset_root_from_modid(modid):
    return "src/main/resources/assets/" + modid