"""
Build the mod

Usage:
    mcmod build
"""
import mcmod

if __name__ == "__main__":
    mcmod.bootstrap_help()
    root = mcmod.find_root()
    name = mcmod.read_mcmod_info(root)["name"]
    archive_base_name = mcmod.archive_base_name_from_name(name)
    version = mcmod.read_version(root)

    mcmod.run_cmd(f"./gradlew build deobfJar", cwd=root)

    print(f"The output jar is at build/libs/{archive_base_name}-{version}.jar")