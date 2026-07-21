# Application icons

All icons derive from the app symbol
`crates/oxidraft_ui/assets/logotype/oxidraft_symbol.png` (a fixed-resolution
raster mark — there is no vector source). Regenerate the artifacts in this
folder after changing it:

```sh
cargo run -p oxidraft_ui --example gen_app_icon
```

This writes:

| File             | Used by                                                       |
|------------------|---------------------------------------------------------------|
| `oxidraft.ico`  | Windows `.exe` (embedded automatically by `build.rs`)         |
| `oxidraft.png`  | 512×512 source for the macOS `.icns` and Linux PNG icon       |

The **window/taskbar** icon is set at runtime in `main.rs` via
`oxidraft_ui::icons::app_icon()`, on every platform. The notes below are only
about the **file/launcher** icon shown by the OS file manager.

## Windows

Nothing to do — `build.rs` embeds `oxidraft.ico` into the executable, so
Explorer, the taskbar and shortcuts show it. (Requires a resource compiler:
the MSVC toolchain's `rc.exe`, or `windres` for the GNU toolchain. If neither is
found the build still succeeds, just without the embedded icon.)

## Linux

ELF binaries don't embed icons; the desktop environment reads a `.desktop`
file plus a themed icon. After `cargo build --release`:

```sh
install -Dm755 target/release/oxidraft        ~/.local/bin/oxidraft
install -Dm644 apps/oxidraft_app/assets/oxidraft.png \
    ~/.local/share/icons/hicolor/512x512/apps/oxidraft.png
install -Dm644 apps/oxidraft_app/assets/oxidraft.desktop \
    ~/.local/share/applications/oxidraft.desktop
update-desktop-database ~/.local/share/applications 2>/dev/null || true
```

(Use `/usr/local/bin` and `/usr/share/...` for a system-wide install.)

## macOS

The Finder icon lives in a `.app` bundle as an `.icns`. Build one from
`oxidraft.png` on a Mac:

```sh
mkdir oxidraft.iconset
for s in 16 32 64 128 256 512; do
    sips -z $s $s   oxidraft.png --out oxidraft.iconset/icon_${s}x${s}.png
    sips -z $((s*2)) $((s*2)) oxidraft.png --out oxidraft.iconset/icon_${s}x${s}@2x.png
done
iconutil -c icns oxidraft.iconset -o oxidraft.icns
```

Then place `oxidraft.icns` in `oxiDRAFT.app/Contents/Resources/` and point
`CFBundleIconFile` at it in `Info.plist`. The easiest path is `cargo-bundle`,
which assembles the bundle and consumes the icon for you.

## Windows code signing

`scripts/codesign-windows.ps1`, wired into the release workflow, signs
`oxidraft.exe` with a **self-signed** Authenticode certificate (subject
`CN=oxiDRAFT, O=fcoltro, C=US`, 10-year validity) if the
`WINDOWS_CODESIGN_PFX_BASE64` / `WINDOWS_CODESIGN_PFX_PASSWORD` repo secrets
are set; otherwise that step is a no-op and the build ships unsigned, same as
before.

**What this does and does not fix:** a self-signed cert has no chain to a
public trusted root, so SmartScreen/UAC still show an "Unknown Publisher"-style
warning for anyone who hasn't explicitly imported the certificate. Signing
only gets you a *consistent, verifiable* publisher identity (the exe's
Digital Signatures tab shows `oxiDRAFT` instead of nothing) and one that a
user can choose to trust on their own machine. It is **not** a substitute for
a CA-issued certificate (e.g. the free SignPath.io OSS program, or Azure
Trusted Signing) if the goal is to make the warning disappear for the general
public downloading a release.

To trust the certificate on a given machine (removes the warning there only):
double-click the exported `.cer` (not the `.pfx` — that has the private key)
→ *Install Certificate* → *Local Machine* → *Trusted Root Certification
Authorities* (or *Trusted People* for a lighter-touch option that only
trusts this exact cert, not anything else it might sign).

The private key (`.pfx`) lives outside the repo — never commit it. To
generate a new one and wire it into CI:

```powershell
$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject "CN=oxiDRAFT, O=fcoltro, C=US" `
    -CertStoreLocation "Cert:\CurrentUser\My" -KeyUsage DigitalSignature -KeyExportPolicy Exportable `
    -HashAlgorithm SHA256 -NotAfter (Get-Date).AddYears(10)
$pwd = Read-Host -AsSecureString "PFX password"
Export-PfxCertificate -Cert $cert -FilePath oxidraft-codesign.pfx -Password $pwd
Export-Certificate -Cert $cert -FilePath oxidraft-codesign.cer   # safe to share/distribute

# then, as GitHub repo secrets:
#   WINDOWS_CODESIGN_PFX_BASE64   = base64 of oxidraft-codesign.pfx
#   WINDOWS_CODESIGN_PFX_PASSWORD = the password above
```
