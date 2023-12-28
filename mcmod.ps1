if ($args.Length -eq 0) {
	$args = @("help")
}
$script = $args[0].TrimStart("-")
$verbose = $false
if (@("V", "verbose").Contains($script)) {
	$verbose = $true
	$script = $args[1].TrimStart("-")
	$args[1] = "-V"
}
# help alias
if (@("h", "?").Contains($script)) {
	$script = "help"
}
# version alias
if (@("v", "ver").Contains($script)) {
	$script = "version"
}
$modules = "$PSScriptRoot\modules"
$script_path = "$PSScriptRoot\scripts\$script.py"
if (![System.IO.File]::Exists($script_path)) {
	echo "unknown subcommand: $script"
	echo "try 'mcmod help'"
	exit 1
}
$args[0] = "$PSScriptRoot\scripts\$script.py"
$old_pythonpath = $env:PYTHONPATH
$env:PYTHONPATH = $modules
try {
	python $args
} catch {
	$env:PYTHONPATH = $old_pythonpath
}