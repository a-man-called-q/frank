# Frank release installer for Windows PowerShell.
#
# The installer deliberately fails closed: an absent, malformed, or mismatched
# SHA256SUMS entry is an error. Set FRANK_RELEASE_BASE_URL to a release asset
# directory when testing locally, and FRANK_INSTALL_DIR to choose the install
# location. No shell code is downloaded or executed from the release.
# HOLD(M6): this script still needs a real Windows PowerShell 5.1/7 run on
# x64 and arm64 before the release workflow can be called release-ready.

function Install-Frank {
  $ErrorActionPreference = "Stop"

  $architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
  switch ($architecture) {
    "X64"   { $targetTriple = "x86_64-pc-windows-msvc" }
    "Arm64" { $targetTriple = "aarch64-pc-windows-msvc" }
    default { throw "frank installer: unsupported Windows architecture: $architecture" }
  }

  $archiveName = "frank-$targetTriple.zip"
  $releaseBase = $env:FRANK_RELEASE_BASE_URL
  if ([string]::IsNullOrWhiteSpace($releaseBase)) {
    $version = if ([string]::IsNullOrWhiteSpace($env:FRANK_VERSION)) { "latest" } else { $env:FRANK_VERSION }
    if ($version -eq "latest") {
      $releaseBase = "https://github.com/JuliusBrussee/frank/releases/latest/download"
    } else {
      $releaseBase = "https://github.com/JuliusBrussee/frank/releases/download/$version"
    }
  }
  $releaseBase = $releaseBase.TrimEnd('/')

  $tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("frank-install-" + [guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Path $tempDir -Force | Out-Null
  try {
    $archivePath = Join-Path $tempDir $archiveName
    $sumsPath = Join-Path $tempDir "SHA256SUMS"
    Invoke-WebRequest -UseBasicParsing -Uri "$releaseBase/$archiveName" -OutFile $archivePath
    Invoke-WebRequest -UseBasicParsing -Uri "$releaseBase/SHA256SUMS" -OutFile $sumsPath

    $expected = @(
      Get-Content -LiteralPath $sumsPath | ForEach-Object {
        if ($_ -match '^\s*([0-9a-fA-F]{64})\s+([^\s]+)\s*$' -and $Matches[2] -eq $archiveName) {
          $Matches[1].ToLowerInvariant()
        }
      }
    )
    if ($expected.Count -ne 1) {
      throw "frank installer: SHA256SUMS has no unique entry for $archiveName"
    }

    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant()
    if ($actual -ne $expected[0]) {
      throw "frank installer: checksum mismatch for $archiveName"
    }

    $extractDir = Join-Path $tempDir "extract"
    Expand-Archive -LiteralPath $archivePath -DestinationPath $extractDir -Force
    $binaryPath = Join-Path $extractDir "frank.exe"
    if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
      throw "frank installer: archive did not contain frank.exe"
    }
    $binaryItem = Get-Item -LiteralPath $binaryPath
    if ($binaryItem.LinkType -or ($binaryItem.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
      throw "frank installer: archive contained a link instead of frank.exe"
    }

    $installDir = if ([string]::IsNullOrWhiteSpace($env:FRANK_INSTALL_DIR)) {
      Join-Path $HOME ".local\bin"
    } else {
      $env:FRANK_INSTALL_DIR
    }
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
    $destination = Join-Path $installDir "frank.exe"
    if (Test-Path -LiteralPath $destination) {
      $existing = Get-Item -LiteralPath $destination -Force
      if ($existing.LinkType -or ($existing.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "frank installer: refusing to replace link $destination"
      }
    }
    Copy-Item -LiteralPath $binaryPath -Destination $destination -Force

    Write-Output "Installed Frank $targetTriple to $destination"
    if (($env:Path -split ';') -notcontains $installDir) {
      Write-Warning "Add $installDir to PATH to invoke frank directly."
    }
  } finally {
    Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
  }
}

# This wrapper also works with `irm ... | iex`, where a top-level param block
# cannot receive arguments reliably.
Install-Frank
