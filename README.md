# mcmod
My CLI tool for MC modding

## Usage
This tool requires `powershell` (you can install PowerShell 7 on non-Windows and alias it to `powershell`)

Clone the repo, then add path to this repo to the PATH environment variable, then run `mcmod` in powershell.

## Dev Setup
Using VSCode, add the following to `.vscode/settings.json`. Replace `<FULL_PATH_TO>` with the full path to the repo.
```json
{
    "python.analysis.extraPaths": [
        "<FULL_PATH_TO>/mcmod/modules"
    ]
}
```