# Build an (unsigned) MSIX for Microsoft Store upload. Run on Windows after `npm run tauri build -- --no-bundle`.
# The Store re-signs the package, so no certificate is needed for submission.
param([string]$Exe = "src-tauri/target/release/topbar-translate.exe", [string]$Out = "dist-msix")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $root
$id = Get-Content packaging/msix/identity.json -Raw -Encoding UTF8 | ConvertFrom-Json
if ($id.name -like "__*") { throw "Fill packaging/msix/identity.json from Partner Center > Product identity first." }
$ver = (Get-Content src-tauri/tauri.conf.json -Raw | ConvertFrom-Json).version
$parts = $ver.Split("."); while ($parts.Count -lt 3) { $parts += "0" }
$msixVer = "$($parts[0]).$($parts[1]).$($parts[2]).0"   # Store requires revision 0

$stage = "$Out/stage"
Remove-Item -Recurse -Force $Out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$stage/Assets" | Out-Null
Copy-Item $Exe "$stage/topbar-translate.exe"
Copy-Item packaging/msix/Assets/* "$stage/Assets/"

$m = Get-Content packaging/msix/AppxManifest.template.xml -Raw -Encoding UTF8
$m = $m.Replace("__NAME__", $id.name).Replace("__PUBLISHER__", $id.publisher).Replace("__VERSION__", $msixVer)
$m = $m.Replace("__PUBLISHER_DISPLAY_NAME__", $id.publisherDisplayName).Replace("__DISPLAY_NAME__", $id.displayName)
[IO.File]::WriteAllText("$root/$stage/AppxManifest.xml", $m, (New-Object Text.UTF8Encoding $false))

# Newest Windows SDK bin folder that has both tools.
$sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Directory |
  Where-Object { $_.Name -match '^10\.' -and (Test-Path "$($_.FullName)\x64\makeappx.exe") -and (Test-Path "$($_.FullName)\x64\makepri.exe") } |
  Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if (-not $sdk) { throw "Windows SDK (makeappx/makepri) not found" }
Write-Host "Using Windows SDK $($sdk.Name)"
$makepri = Join-Path $sdk.FullName "x64\makepri.exe"
$makeappx = Join-Path $sdk.FullName "x64\makeappx.exe"
Push-Location $stage
& $makepri createconfig /cf priconfig.xml /dq zh-CN /o | Out-Null
& $makepri new /pr . /cf priconfig.xml /of resources.pri /o
if ($LASTEXITCODE) { throw "makepri failed" }
Remove-Item priconfig.xml
Pop-Location
$name = "TopBarTranslate_${msixVer}_x64.msix"
& $makeappx pack /d $stage /p "$Out/$name" /o
if ($LASTEXITCODE) { throw "makeappx failed" }
Write-Host "Built $Out/$name"
