# Get/Set the info of the current mod
# usage: mcmod info [<field> <value>]

# info fields:
# name: the name of the mod (from mcmod.info)
# desc[r[iption]]: the description of the mod (from mcmod.info)
# credits: the credits of the mod (from mcmod.info)
# url: the url of the mod (from mcmod.info)
# version: the version of the mod (from build.gradle)

# derived fields:
# modid: from name
# archive base name: from name
# group: from name
import shutil
import os

def edit_java_package(args):
    file, package = args
    with open(file, "r", encoding="utf-8") as f:
        f.readline()
        with open(file+".tmp", "w", encoding="utf-8") as f2:
            f2.write(f"package {package};\n")
            shutil.copyfileobj(f, f2)
    os.remove(file)
    os.rename(file+".tmp", file)

    return file


if __name__ == "__main__":
    import json
    import sys
    import multiprocessing

    if len(sys.argv) != 1:
        if len(sys.argv) != 3:
            print("usage: mcmod info [<field> <value>]")
            exit(1)
        SET = True
    else:
        SET = False

    MCMOD_INFO = "src/main/resources/mcmod.info"
    BUILD_GRADLE = "build.gradle"

    # read fields
    with open(MCMOD_INFO, "r", encoding="utf-8") as f:
        info = json.load(f)[0]

    name = info["name"]
    description = info["description"]
    credits = info["credits"]
    url = info["url"]

    with open(BUILD_GRADLE, "r", encoding="utf-8") as f:
        for line in f:
            if line.startswith("version"):
                version = line.split("=")[1].strip().strip("\"")
                break

    old_modid = name.lower().replace(" ", "")

    if SET:
        if sys.argv[1] == "name":
            name = sys.argv[2]
        elif sys.argv[1] == "description" or sys.argv[1] == "desc":
            description = sys.argv[2]
        elif sys.argv[1] == "credits":
            credits = sys.argv[2]
        elif sys.argv[1] == "url":
            url = sys.argv[2]
        elif sys.argv[1] == "version":
            version = sys.argv[2]
        else:
            print("not valid field: " + sys.argv[1])
            exit(1)

    modid = name.lower().replace(" ", "")
    achive_base_name = name.lower().replace(" ", "-")
    group = "com.piston.mc." + modid



    if not SET:
        print("name:        " + name)
        print("description: " + description)
        print("credits:     " + credits)
        print("url:         " + url)
        print("version:     " + version)
        print("[modid]:     " + modid)
        print("[archive]:   " + achive_base_name)
        print("[group]:     " + group)
        exit(0)

    # write fields
    info["name"] = name
    info["description"] = description
    info["credits"] = credits
    info["url"] = url
    info["modid"] = modid

    MODINFO_PREFIX = "src/main/java/com/piston/mc/"
    with open(MCMOD_INFO, "w", encoding="utf-8") as f:
        json.dump([info], f, indent=4)

    with open(BUILD_GRADLE, "r", encoding="utf-8") as f:
        lines = [x for x in f]

    with open(BUILD_GRADLE, "w", encoding="utf-8") as f:
        for line in lines:
            if line.startswith("version"):
                f.write("version = \"" + version + "\"\n")
            elif line.startswith("group"):
                f.write("group = \"" + group + "\"\n")
            elif line.startswith("archivesBaseName"):
                f.write("archivesBaseName = \"" + achive_base_name + "\"\n")
            else:
                f.write(line)
        f.flush()

    if old_modid != modid:
        modinfo = MODINFO_PREFIX + modid
        os.rename(MODINFO_PREFIX + old_modid, modinfo)
        ASSET_PREFIX = "src/main/resources/assets/"
        os.rename(ASSET_PREFIX + old_modid, ASSET_PREFIX + modid)
        modinfo_java = modinfo + "/ModInfo.java"
        with open(modinfo_java, "w", encoding="utf-8") as f:
            f.write(f"""package com.piston.mc.{modid};
// This file is automatically generated with mcmod.py
// Do not edit this file directly
public interface ModInfo {{
    String Id = "{modid}";
    String Version = "{version}";
}}
    """)
            f.flush()
            
        tasks = []
        
        for root, dirs, files in os.walk(modinfo):
            package = "com.piston.mc." + root[len(MODINFO_PREFIX):].replace("/", ".").replace("\\", ".").strip(".")
            for f in files:
                if f.endswith(".java"):
                    file = os.path.join(root, f)
                    tasks.append((file, package))
        with multiprocessing.Pool() as pool:
            for file in pool.imap_unordered(edit_java_package, tasks):
                print("edited " + file)

        



        