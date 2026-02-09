param(
  [string]$Bake = "build_debug"
)

$rootDir = Resolve-Path (Join-Path $PSScriptRoot "../..")
$steelBin = if ($env:STEEL_BIN) { $env:STEEL_BIN } else { "steel" }

New-Item -ItemType Directory -Force -Path (Join-Path $rootDir "target/out") | Out-Null
& $steelBin run --root $rootDir --file (Join-Path $rootDir "examples/swift/steelconf") --bake $Bake
