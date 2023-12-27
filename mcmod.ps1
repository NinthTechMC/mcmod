if ($args.Length -eq 0) {
	echo "usage: mcmod <script> <args>"
	echo "See $PSScriptRoot\scripts for what scripts are available"
	exit 1
}
$script = $args[0]
$args[0] = "$PSScriptRoot\scripts\$script.py"
$args_formated = $args | % { "`"$_`"" }
echo "[mcmod] python $args_formated"
python $args