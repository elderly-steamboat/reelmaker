<#
.SYNOPSIS
  Builds a distributable ReelMaker bundle for Windows (x86_64).

.DESCRIPTION
  1. Fetches libmpv (dev build) into vendor\mpv-dev and generates the MSVC import library mpv.lib.
  2. Fetches static GPL ffmpeg/ffprobe into vendor\ffmpeg.
  3. cargo build --release
  4. Assembles dist\reelmaker-windows-x86_64\ (exe, libmpv-2.dll, ffmpeg.exe, ffprobe.exe) and zips it.
  5. Smoke test: runs the bundled exe with --version from a clean directory.

  Requires: Rust (MSVC toolchain) and Visual Studio Build Tools (C++ workload).
  The ffmpeg builds are GPL (libx264): fine for personal use, review before public distribution.
#>
param([switch]$SkipDownload)

$ErrorActionPreference = "Stop"
$root   = Resolve-Path (Join-Path $PSScriptRoot "..")
$vendor = Join-Path $root "vendor"
$mpvDir = Join-Path $vendor "mpv-dev"
$ffDir  = Join-Path $vendor "ffmpeg"
$name   = "reelmaker-windows-x86_64"
$dist   = Join-Path $root "dist"
$bundle = Join-Path $dist $name

function Download($url, $out) {
  Write-Host "Downloading $url"
  Invoke-WebRequest -Uri $url -OutFile $out -UseBasicParsing
}

# ---- 1. libmpv -------------------------------------------------------------------------------
if (-not (Test-Path (Join-Path $mpvDir "mpv.lib"))) {
  if ($SkipDownload) { throw "vendor\mpv-dev\mpv.lib missing and -SkipDownload given" }
  New-Item -ItemType Directory -Force $mpvDir | Out-Null
  $rel = Invoke-RestMethod "https://api.github.com/repos/shinchiro/mpv-winbuild-cmake/releases/latest" -Headers @{ "User-Agent" = "reelmaker-build" }
  $asset = $rel.assets | Where-Object { $_.name -match '^mpv-dev-x86_64-\d' } | Select-Object -First 1
  if (-not $asset) { throw "no mpv-dev-x86_64 asset found in the latest shinchiro release" }
  $archive = Join-Path $vendor "mpv-dev.7z"
  Download $asset.browser_download_url $archive
  tar -xf $archive -C $mpvDir
  if ($LASTEXITCODE -ne 0) { throw "could not extract $archive (needs a tar that reads .7z, e.g. Windows 10+ bsdtar)" }

  # MSVC cannot link the MinGW libmpv.dll.a; generate mpv.lib from the DLL's exports.
  $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
  $vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
  if (-not $vsPath) { throw "Visual Studio C++ build tools not found" }
  $vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
  $exports = cmd /c "`"$vcvars`" >nul && dumpbin /exports `"$mpvDir\libmpv-2.dll`""
  $names = $exports | ForEach-Object { if ($_ -match '^\s+\d+\s+[0-9A-F]+\s+[0-9A-F]{8}\s+(mpv_\S+)') { $Matches[1] } }
  if (-not $names) { throw "no mpv_* exports found in libmpv-2.dll" }
  @("LIBRARY libmpv-2.dll", "EXPORTS") + $names | Set-Content -Encoding ascii (Join-Path $mpvDir "mpv.def")
  cmd /c "`"$vcvars`" >nul && lib /nologo /def:`"$mpvDir\mpv.def`" /machine:x64 /out:`"$mpvDir\mpv.lib`""
  if (-not (Test-Path (Join-Path $mpvDir "mpv.lib"))) { throw "failed to generate mpv.lib" }
}

# ---- 2. ffmpeg -------------------------------------------------------------------------------
if (-not (Test-Path (Join-Path $ffDir "ffmpeg.exe"))) {
  if ($SkipDownload) { throw "vendor\ffmpeg missing and -SkipDownload given" }
  New-Item -ItemType Directory -Force $ffDir | Out-Null
  $zip = Join-Path $vendor "ffmpeg.zip"
  Download "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip" $zip
  $tmp = Join-Path $vendor "ffmpeg-extract"
  if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
  Expand-Archive $zip -DestinationPath $tmp
  $bin = Get-ChildItem $tmp -Recurse -Filter ffmpeg.exe | Select-Object -First 1
  Copy-Item $bin.FullName $ffDir
  Copy-Item (Join-Path $bin.DirectoryName "ffprobe.exe") $ffDir
  Remove-Item -Recurse -Force $tmp
}

# ---- 3. cargo build --------------------------------------------------------------------------
$env:MPV_LIB_DIR = $mpvDir
Push-Location $root
try {
  cargo build --release
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally { Pop-Location }

# ---- 4. bundle -------------------------------------------------------------------------------
if (Test-Path $bundle) { Remove-Item -Recurse -Force $bundle }
New-Item -ItemType Directory -Force $bundle | Out-Null
Copy-Item (Join-Path $root "target\release\reelmaker.exe") $bundle
Copy-Item (Join-Path $mpvDir "libmpv-2.dll") $bundle
Copy-Item (Join-Path $ffDir "ffmpeg.exe") $bundle
Copy-Item (Join-Path $ffDir "ffprobe.exe") $bundle
Copy-Item (Join-Path $root "README.md") $bundle
$zipPath = Join-Path $dist "$name.zip"
if (Test-Path $zipPath) { Remove-Item $zipPath }
Compress-Archive -Path $bundle -DestinationPath $zipPath
Write-Host "Created $zipPath"

# ---- 4b. installer ---------------------------------------------------------------------------
# One double-clickable ReelMaker-Setup-<version>.exe (per-user install, no admin rights needed).
$version = [regex]::Match((Get-Content (Join-Path $root "Cargo.toml") -Raw), '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
$iscc = @(
  (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"),
  (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"),
  (Join-Path $env:ProgramFiles "Inno Setup 6\ISCC.exe")
) | Where-Object { Test-Path $_ } | Select-Object -First 1
$setupExe = Join-Path $dist "ReelMaker-Setup-$version.exe"
if (-not $iscc) {
  Write-Warning "Inno Setup not found - skipping the installer. Install it with: winget install JRSoftware.InnoSetup"
} else {
  if (Test-Path $setupExe) { Remove-Item $setupExe }
  & $iscc /Qp "/DAppVersion=$version" (Join-Path $PSScriptRoot "installer.iss")
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $setupExe)) { throw "Inno Setup failed" }
  Write-Host "Created $setupExe"

  # Install silently into a temp dir, run the installed app, then uninstall again.
  $inst = Join-Path ([System.IO.Path]::GetTempPath()) ("reelmaker-install-" + [guid]::NewGuid())
  $p = Start-Process -FilePath $setupExe -ArgumentList "/VERYSILENT","/SUPPRESSMSGBOXES","/NORESTART","/SP-","/DIR=`"$inst`"" -Wait -PassThru
  if ($p.ExitCode -ne 0) { throw "silent install failed (exit $($p.ExitCode))" }
  foreach ($f in "reelmaker.exe","libmpv-2.dll","ffmpeg.exe","ffprobe.exe","unins000.exe") {
    if (-not (Test-Path (Join-Path $inst $f))) { throw "installer did not install $f" }
  }
  $vf = Join-Path $inst "version.txt"
  $v = Start-Process -FilePath (Join-Path $inst "reelmaker.exe") -ArgumentList "--version" -Wait -NoNewWindow -PassThru -RedirectStandardOutput $vf
  $vout = (Get-Content $vf -ErrorAction SilentlyContinue) -join " "
  if ($v.ExitCode -ne 0 -or $vout -notmatch "reelmaker") { throw "installed app failed to start: '$vout'" }
  Write-Host "Installer smoke test OK: $vout"
  $u = Start-Process -FilePath (Join-Path $inst "unins000.exe") -ArgumentList "/VERYSILENT","/SUPPRESSMSGBOXES","/NORESTART" -Wait -PassThru
  Start-Sleep -Seconds 2
  if (Test-Path (Join-Path $inst "reelmaker.exe")) { throw "uninstall left reelmaker.exe behind" }
  Remove-Item -Recurse -Force $inst -ErrorAction SilentlyContinue
}

# ---- 5. smoke test ---------------------------------------------------------------------------
$test = Join-Path ([System.IO.Path]::GetTempPath()) ("reelmaker-smoke-" + [guid]::NewGuid())
New-Item -ItemType Directory $test | Out-Null
Expand-Archive $zipPath -DestinationPath $test
$exe = Join-Path $test "$name\reelmaker.exe"
# Strip any ffmpeg/mpv directories from PATH so only the bundle can satisfy the loader.
$savedPath = $env:Path
$env:Path = ($env:Path -split ';' | Where-Object { $_ -and -not (Test-Path (Join-Path $_ "libmpv-2.dll")) -and -not (Test-Path (Join-Path $_ "ffmpeg.exe")) }) -join ';'
try {
  $outFile = Join-Path $test "version.txt"
  $proc = Start-Process -FilePath $exe -ArgumentList "--version" -Wait -NoNewWindow -PassThru -RedirectStandardOutput $outFile
  $out = (Get-Content $outFile -ErrorAction SilentlyContinue) -join " "
  if ($proc.ExitCode -ne 0 -or $out -notmatch "reelmaker") { throw "smoke test failed: '$out' (exit $($proc.ExitCode))" }
  Write-Host "Smoke test OK: $out"
  & (Join-Path (Join-Path $test $name) "ffmpeg.exe") -version | Select-Object -First 1
} finally {
  $env:Path = $savedPath
  Start-Sleep -Seconds 1   # let ffmpeg.exe release its file lock (scanners can hold it briefly)
  Remove-Item -Recurse -Force $test -ErrorAction SilentlyContinue   # temp cleanup must never fail the build
}
