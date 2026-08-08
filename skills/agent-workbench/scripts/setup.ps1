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
$runtimeParent = Join-Path $ProjectRoot ".agent-workbench"
$destination = Join-Path $runtimeParent "bin"
$candidate = Join-Path $runtimeParent ".bin.next"
$previous = Join-Path $runtimeParent ".bin.previous"
$activationPending = Join-Path $runtimeParent ".bin.activation-pending"
$runtime = Join-Path $destination "agent-workbench.exe"
$temporary = ""
$requiredFiles = @(
  "LICENSE-BLAKE3-APACHE-2.0",
  "LICENSE-BLAKE3-APACHE-2.0-LLVM",
  "LICENSE-BLAKE3-CC0-1.0",
  "LICENSE-Blake3-lean",
  "LICENSE-agent-workbench",
  "LICENSE-elan-APACHE",
  "LICENSE-elan-MIT",
  "LICENSE-lean4",
  "LICENSE-leansqlite",
  "LICENSES-lean4",
  "README.md",
  "agent-workbench.exe",
  "docs/assurance.md",
  "docs/concepts.md",
  "docs/getting-started.md",
  "docs/index.md",
  "docs/installation.md",
  "docs/operation-reference.md",
  "docs/recovery.md",
  "docs/releases.md",
  "docs/reviews.md",
  "docs/state-reference.md",
  "docs/workflow.md",
  "elan.exe",
  "skill/agent-workbench/SKILL.md",
  "skill/agent-workbench/agents/openai.yaml",
  "skill/agent-workbench/release-version",
  "skill/agent-workbench/scripts/setup.ps1",
  "skill/agent-workbench/scripts/setup.sh"
)
$requiredDirectories = @(
  ".",
  "docs",
  "skill",
  "skill/agent-workbench",
  "skill/agent-workbench/agents",
  "skill/agent-workbench/scripts"
)

function Test-RuntimeBundle([string]$BundleRoot) {
  if (-not (Test-Path -LiteralPath $BundleRoot -PathType Container)) { return $false }
  $marker = Join-Path $BundleRoot "skill/agent-workbench/release-version"
  if (-not (Test-Path -LiteralPath $marker -PathType Leaf)) { return $false }
  if ((Get-Content -LiteralPath $marker -Raw).Trim() -ne $releaseVersion) { return $false }
  $entries = @(Get-ChildItem -LiteralPath $BundleRoot -Force -Recurse)
  if ($entries | Where-Object {
      ($_.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0
    }) { return $false }
  $actualFiles = @($entries | Where-Object { -not $_.PSIsContainer } | ForEach-Object {
    [System.IO.Path]::GetRelativePath($BundleRoot, $_.FullName).Replace('\', '/')
  } | Sort-Object)
  $actualDirectories = @(".") + @($entries | Where-Object { $_.PSIsContainer } | ForEach-Object {
    [System.IO.Path]::GetRelativePath($BundleRoot, $_.FullName).Replace('\', '/')
  } | Sort-Object)
  if ((Compare-Object $requiredFiles $actualFiles).Count -ne 0) { return $false }
  if ((Compare-Object $requiredDirectories $actualDirectories).Count -ne 0) { return $false }
  return $true
}

function Remove-PathIfPresent([string]$Path) {
  if (Test-Path -LiteralPath $Path) { Remove-Item -LiteralPath $Path -Recurse -Force }
}

function Restore-RuntimeSwap {
  if (Test-Path -LiteralPath $activationPending) {
    Remove-PathIfPresent $destination
    if (Test-Path -LiteralPath $previous) {
      Move-Item -LiteralPath $previous -Destination $destination
    }
    Remove-PathIfPresent $activationPending
  } elseif (Test-Path -LiteralPath $previous) {
    if ((Test-Path -LiteralPath $destination) -and (Test-RuntimeBundle $destination)) {
      Remove-PathIfPresent $previous
    } else {
      Remove-PathIfPresent $destination
      Move-Item -LiteralPath $previous -Destination $destination
    }
  }
  Remove-PathIfPresent $candidate
}

if ((Test-Path -LiteralPath $previous) -or (Test-Path -LiteralPath $candidate) -or
    (Test-Path -LiteralPath $activationPending)) {
  New-Item -ItemType Directory -Force -Path $runtimeParent | Out-Null
  Restore-RuntimeSwap
}
$needsInstall = -not (Test-RuntimeBundle $destination)

try {
  if ($needsInstall) {
    $temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("agent-workbench-setup-" + [guid]::NewGuid())
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

    New-Item -ItemType Directory -Force -Path $runtimeParent | Out-Null
    Remove-PathIfPresent $candidate
    New-Item -ItemType Directory -Path $candidate | Out-Null
    Expand-Archive $archivePath $candidate
    if (-not (Test-RuntimeBundle $candidate)) {
      throw "Downloaded Agent Workbench archive is not a complete release bundle"
    }

    Remove-PathIfPresent $previous
    if (Test-Path -LiteralPath $destination) {
      Move-Item -LiteralPath $destination -Destination $previous
    }
    New-Item -ItemType File -Path $activationPending | Out-Null
    try {
      Move-Item -LiteralPath $candidate -Destination $destination
    } catch {
      throw "Failed to replace the Agent Workbench runtime bundle: $_"
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
  if ($needsInstall) {
    Remove-PathIfPresent $activationPending
    Remove-PathIfPresent $previous
  }
} finally {
  if (Test-Path -LiteralPath $activationPending) {
    Remove-PathIfPresent $destination
    if (Test-Path -LiteralPath $previous) {
      Move-Item -LiteralPath $previous -Destination $destination
    }
    Remove-PathIfPresent $activationPending
  } else {
    if ((-not (Test-Path -LiteralPath $destination)) -and
        (Test-Path -LiteralPath $previous)) {
      Move-Item -LiteralPath $previous -Destination $destination
    }
    if ((Test-Path -LiteralPath $destination) -and
        (Test-Path -LiteralPath $previous)) {
      Remove-PathIfPresent $previous
    }
  }
  Remove-PathIfPresent $candidate
  if ($temporary -and (Test-Path $temporary)) {
    Remove-Item -Recurse -Force $temporary
  }
}
