"""
Print help message
"""
import mcmod

if __name__ == "__main__":
    mcmod.bootstrap()
    print("mcmod - CLI tool for Minecraft modding")
    print()
    print("Usage:")
    print("    mcmod [-V/--verbose] COMMAND [args...]")
    print()
    print("Commands:")
    mcmod.print_subcommands()
    print()
    print("(run `mcmod COMMAND help` for information about a specific command)")
