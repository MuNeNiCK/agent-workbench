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

try {
  New-Item -ItemType Directory -Path $temporary | Out-Null
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
  $destination = Join-Path $ProjectRoot ".agent-workbench/bin"
  New-Item -ItemType Directory -Force -Path $destination | Out-Null
  Expand-Archive -Force $archivePath $destination
  & (Join-Path $destination "agent-workbench.exe") --project $ProjectRoot init
} finally {
  if (Test-Path $temporary) { Remove-Item -Recurse -Force $temporary }
}
