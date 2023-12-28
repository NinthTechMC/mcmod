"""
Init a new mod project

Usage:
    mcmod init TEMPLATE_PATH MOD_NAME

Args:
    TEMPLATE_PATH   the path to the template folder
    MOD_NAME        the name of the mod (captialized and spaced)

"""
import mcmod
import sys
import shutil
import os
import mcmod

if __name__ == "__main__":
    mcmod.bootstrap_help()
    if len(sys.argv) != 3:
        mcmod.print_help()
        exit(1)

    template_path = sys.argv[1]
    mod_name = sys.argv[2]

    mod_folder_name = mcmod.project_name_from_name(mod_name)
    mod_folder_name = os.path.abspath(mod_folder_name)

    if os.path.exists(mod_folder_name):
        print(f"folder {mod_folder_name} already exists")
        exit(1)
    
    print(f"copying {template_path} to {mod_folder_name}")
    shutil.copytree(template_path, mod_folder_name)

    def run_cmd(cmd):
        mcmod.run_cmd(cmd, cwd=mod_folder_name)

    run_cmd(f"mcmod info name \"{mod_name}\"")

    print("setting up workspace")
    run_cmd("./gradlew setupDecompWorkspace")

    run_cmd("mcmod eclipse")

    print("setting up git repo")
    run_cmd("git init")

    GIT_IGNORE = ".gitignore"
    IGNROE = ["/build", "/run", "/.gradle", "/.settings", "/bin", "/.vscode", "/libs", "/eclipse", "/.classpath", "/.project" ]
    with open(os.path.join(mod_folder_name, GIT_IGNORE), "a", encoding="utf-8") as f:
        for i in IGNROE:
            f.write(i+"\n")

    os.makedirs(os.path.join(mod_folder_name, "libs"), exist_ok=True)

    print("trying to build for the first time")
    run_cmd("mcmod build")

    if input("Launch client now? (y/n) ") == "y":
        run_cmd("mcmod run client")

    print()
    print("Next:")
    print("  1. Import project in eclipse: File -> Import -> Existing Projects into Workspace")
    print("  2. Put dependency JARs in libs/")
    print("  3. Run with `mcmod run client`")