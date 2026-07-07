# Signs a Windows executable with the project's self-signed Authenticode
# certificate (see apps/oxidraft_app/assets/PACKAGING.md for why this is
# self-signed and what it does/doesn't fix re: SmartScreen). Used by
# .github/workflows/release.yml; the base64 PFX + password come from repo
# secrets and are never written to disk outside a temp file this script
# deletes when done.
param(
    [Parameter(Mandatory)] [string]$Path,
    [Parameter(Mandatory)] [string]$PfxBase64,
    [Parameter(Mandatory)] [string]$PfxPassword
)
$ErrorActionPreference = "Stop"

$signtool = Get-ChildItem -Path "C:\Program Files (x86)\Windows Kits","C:\Program Files\Windows Kits" `
    -Filter "signtool.exe" -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "x64" } |
    Select-Object -Last 1 -ExpandProperty FullName
if (-not $signtool) {
    throw "signtool.exe not found (expected the Windows SDK to be preinstalled on this runner)"
}

$pfxPath = Join-Path ([System.IO.Path]::GetTempPath()) "oxidraft-codesign-$([guid]::NewGuid()).pfx"
try {
    [IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($PfxBase64))

    # Self-signed cert: timestamping still works (any public RFC3161 TSA will
    # timestamp regardless of who issued the signing cert), so the signature
    # stays valid after the 10-year cert expires even if it's never renewed.
    & $signtool sign /f $pfxPath /p $PfxPassword /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $Path
    if ($LASTEXITCODE -ne 0) {
        throw "signtool sign failed (exit $LASTEXITCODE)"
    }
} finally {
    Remove-Item -Path $pfxPath -Force -ErrorAction SilentlyContinue
}

Write-Output "Signed: $Path"
