param(
    [Parameter(Mandatory)][string]$Tag,
    [string]$TargetDirectory = 'target',
    [string]$Destination = 'dist'
)

$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Destination | Out-Null

$checksums = foreach ($build in @(
    @{ Arch = 'x64'; Target = 'x86_64-pc-windows-msvc' }
    @{ Arch = 'arm64'; Target = 'aarch64-pc-windows-msvc' }
)) {
    $exe = Join-Path $TargetDirectory "$($build.Target)/release/caffeinate.exe"
    $name = "caffeinate-$Tag-windows-$($build.Arch).zip"
    $zip = Join-Path $Destination $name
    Compress-Archive -LiteralPath $exe -DestinationPath $zip -Force
    '{0}  {1}' -f (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant(), $name
}
$checksums | Set-Content -LiteralPath (Join-Path $Destination 'SHASUMS256.txt')
