# Windows packaging

## Manifest

`ted.exe` is a console subsystem binary, as Postino's (#282), so shells wait for its commands.
Right after the release build, `merge-manifest.ps1` merges `console.manifest`
(`consoleAllocationPolicy` `detached`, so a double click opens no console window on Windows 11
24H2 or later) into the manifest gpui embeds, with `mt.exe` from the Windows SDK. It runs before
the installer and the zip, which package the merged exe:

```powershell
powershell -File packaging\windows\merge-manifest.ps1 target\x86_64-pc-windows-msvc\release\ted.exe
```

## Installer

`ted.iss` is the Inno Setup script of the installer, built by `release.yml`.
