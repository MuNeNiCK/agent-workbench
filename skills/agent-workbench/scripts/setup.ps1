param(
  [string]$ProjectRoot = (Get-Location).Path,
  [string]$LocalArchive = "",
  [string]$LocalChecksum = ""
)

$ErrorActionPreference = "Stop"
$repository = "https://github.com/MuNeNiCK/agent-workbench"
$releaseVersion = (Get-Content (Join-Path $PSScriptRoot "../release-version") -Raw).Trim()
if ($releaseVersion -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') {
  throw "Invalid Agent Workbench release version"
}
$archive = "agent-workbench-windows-x86_64.zip"
$temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("agent-workbench-setup-" + [guid]::NewGuid())
$destination = Join-Path $ProjectRoot ".agent-workbench/bin"
$runtime = Join-Path $destination "agent-workbench.exe"
$runtimeRelease = Join-Path $destination "skill/agent-workbench/release-version"
$needsInstall = -not (Test-Path $runtime) -or -not (Test-Path $runtimeRelease)
if (-not $needsInstall) {
  $needsInstall = ((Get-Content $runtimeRelease -Raw).Trim() -ne $releaseVersion)
}

try {
  New-Item -ItemType Directory -Path $temporary | Out-Null
  if ($needsInstall) {
    $archivePath = Join-Path $temporary $archive
    $checksumPath = "$archivePath.sha256"
    if ($LocalArchive -or $LocalChecksum) {
      if (-not $LocalArchive -or -not $LocalChecksum) {
        throw "LocalArchive and LocalChecksum must be provided together"
      }
      Copy-Item $LocalArchive $archivePath
      Copy-Item $LocalChecksum $checksumPath
    } else {
      Invoke-WebRequest "$repository/releases/download/$releaseVersion/$archive" -OutFile $archivePath
      Invoke-WebRequest "$repository/releases/download/$releaseVersion/$archive.sha256" -OutFile $checksumPath
      & gh attestation verify $archivePath `
        --repo MuNeNiCK/agent-workbench `
        --signer-workflow MuNeNiCK/agent-workbench/.github/workflows/release.yml `
        --deny-self-hosted-runners `
        --source-ref "refs/tags/$releaseVersion" | Out-Null
      if ($LASTEXITCODE -ne 0) { throw "Agent Workbench archive provenance verification failed" }
    }
    $expected = ((Get-Content $checksumPath -Raw) -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash $archivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $expected) { throw "Agent Workbench archive checksum mismatch" }
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    Expand-Archive -Force $archivePath $destination
    if ((Get-Content $runtimeRelease -Raw).Trim() -ne $releaseVersion) {
      throw "Installed Agent Workbench runtime identity differs from the Skill release"
    }
  }
  if (Test-Path (Join-Path $ProjectRoot ".agent-workbench/state.db")) {
    $contextOutput = (& $runtime --project $ProjectRoot context 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -eq 0) {
      $contextOutput
    } elseif ($contextOutput -match 'unsupported schema revision 1; expected 2') {
      & $runtime --project $ProjectRoot init
      if ($LASTEXITCODE -ne 0) { throw "Agent Workbench migration failed" }
    } else {
      throw $contextOutput
    }
  } else {
    & $runtime --project $ProjectRoot init
    if ($LASTEXITCODE -ne 0) { throw "Agent Workbench initialization failed" }
  }
} finally {
  if (Test-Path $temporary) { Remove-Item -Recurse -Force $temporary }
}
