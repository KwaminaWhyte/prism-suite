# package-windows.ps1 -AppName Pigment -Bin pigment-gpui
param(
    [Parameter(Mandatory)][string]$AppName,
    [Parameter(Mandatory)][string]$Bin,
    [string]$Version = ""
)

$APP_LOWER = $AppName.ToLower()
$BRANDING  = "assets\branding"

if (-not $Version) {
    $Version = (Select-String -Path "Cargo.toml" -Pattern '^version\s*=\s*"(.+)"').Matches[0].Groups[1].Value
}

Write-Host "Building $AppName $Version (release)..."
cargo build --release -p $Bin
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$DistDir = "dist\$AppName-$Version-windows"
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

Copy-Item "target\release\$Bin.exe" "$DistDir\$AppName.exe"

$MasterPng = "$BRANDING\${APP_LOWER}-master.png"
if ((Get-Command magick -ErrorAction SilentlyContinue) -and (Test-Path $MasterPng)) {
    magick convert $MasterPng -define icon:auto-resize=256,128,64,48,32,16 "$DistDir\$AppName.ico"
    Write-Host "Generated $AppName.ico"
}

if (Test-Path "README.md") { Copy-Item "README.md" "$DistDir\" }

$ZipPath = "dist\$AppName-$Version-windows-x64.zip"
Compress-Archive -Path "$DistDir\*" -DestinationPath $ZipPath -Force

Write-Host ""
Write-Host "Done:"
Write-Host "  Folder : $DistDir"
Write-Host "  Zip    : $ZipPath"
