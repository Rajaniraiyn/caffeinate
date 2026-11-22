param([string]$Tag)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

$manifest = 'Cargo.toml'
$metadata = cargo metadata --locked --no-deps --format-version 1 | ConvertFrom-Json
$current = $metadata.packages[0].version
$stableTag = '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'

if ($Tag) {
    if ($Tag -cnotmatch $stableTag -or $Tag -cne "v$current") {
        throw "Release tag '$Tag' must match the stable package version v$current."
    }
    return $Tag
}

$latest = git tag --list --sort=-version:refname 'v*' |
    Where-Object { $_ -cmatch $stableTag } | Select-Object -First 1
$next = [version]$current
if ($latest) {
    $previous = [version]$latest.Substring(1)
    if ($next -le $previous) {
        $next = [version]::new($previous.Major, $previous.Minor, $previous.Build + 1)
    }
}

if ($current -ne "$next") {
    $versionLine = [regex]::new("(?m)^version = `"$([regex]::Escape($current))`"(?=\r?$)")
    $contents = Get-Content -Raw -LiteralPath $manifest
    $updated = $versionLine.Replace($contents, "version = `"$next`"", 1)
    if ($updated -ceq $contents) {
        throw "Could not update the package version in $manifest."
    }
    Set-Content -LiteralPath $manifest -Value $updated -NoNewline
    cargo update --workspace | Out-Host
}

"v$next"
