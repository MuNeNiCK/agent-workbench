param(
  [string]$ProjectRoot = (Get-Location).Path,
  [string]$LocalArchive = "",
  [string]$LocalChecksum = ""
)

$ErrorActionPreference = "Stop"
$repository = "https://github.com/MuNeNiCK/agent-workbench"
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
    Invoke-WebRequest "$repository/releases/latest/download/$archive" -OutFile $archivePath
    Invoke-WebRequest "$repository/releases/latest/download/$archive.sha256" -OutFile $checksumPath
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
