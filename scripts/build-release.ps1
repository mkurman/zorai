#Requires -Version 5.1
<#
.SYNOPSIS
    zorai release build for Windows (PowerShell)

.DESCRIPTION
    Builds Rust binaries, frontend, and Electron app with optional code signing.
    Supports PFX file signing, certificate store signing, and Azure SignTool.

.PARAMETER Sign
    Enable code signing of all executables.

.PARAMETER SignTool
    Signing method: "signtool" (default), "azure", or "pfx".
    - signtool: Uses certificate thumbprint from store
    - pfx: Uses PFX file (set ZORAI_SIGN_CERT and ZORAI_SIGN_PASSWORD)
    - azure: Uses Azure Trusted Signing (set AZURE_* env vars)

.PARAMETER Thumbprint
    Certificate thumbprint for signtool signing. Overrides ZORAI_SIGN_THUMBPRINT env var.

.PARAMETER CertFile
    Path to PFX certificate file. Overrides ZORAI_SIGN_CERT env var.

.PARAMETER CertPassword
    PFX certificate password. Overrides ZORAI_SIGN_PASSWORD env var.

.PARAMETER SkipRust
    Skip Rust compilation (use existing binaries).

.PARAMETER SkipFrontend
    Skip frontend build (use existing dist/).

.PARAMETER SkipElectron
    Skip Electron packaging.

.PARAMETER Target
    Rust target triple. Default: native.

.EXAMPLE
    .\scripts\build-release.ps1
    Build without signing.

.EXAMPLE
    .\scripts\build-release.ps1 -Sign -SignTool pfx -CertFile .\cert.pfx -CertPassword "secret"
    Build and sign with a PFX certificate.

.EXAMPLE
    .\scripts\build-release.ps1 -Sign -Thumbprint "ABCD1234..."
    Build and sign with certificate from Windows certificate store.
#>

[CmdletBinding()]
param(
    [switch]$Sign,
    [ValidateSet("signtool", "azure", "pfx")]
    [string]$SignTool = "signtool",
    [string]$Thumbprint,
    [string]$CertFile,
    [string]$CertPassword,
    [switch]$SkipRust,
    [switch]$SkipFrontend,
    [switch]$SkipElectron,
    [string]$Target
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$OutDir = Join-Path $ProjectRoot "dist-release"
$FrontendDir = Join-Path $ProjectRoot "frontend"

$env:ZORAI_LOG = "error"
$env:ZORAI_TUI_LOG = "error"
$env:ZORAI_GATEWAY_LOG = "error"
$env:RUST_LOG = "error"

# ─────────────────────────────────────────────────────
# Helpers
# ─────────────────────────────────────────────────────

function Write-Step($num, $total, $msg) {
    Write-Host "`n[$num/$total] $msg" -ForegroundColor Cyan
}

function Write-Ok($msg) {
    Write-Host "  $msg" -ForegroundColor Green
}

function Write-Warn($msg) {
    Write-Host "  WARNING: $msg" -ForegroundColor Yellow
}

function Invoke-NpmCiWithRetries {
    & npm config set fetch-retries 5
    if ($LASTEXITCODE -ne 0) { throw "npm config failed" }
    & npm config set fetch-retry-mintimeout 20000
    if ($LASTEXITCODE -ne 0) { throw "npm config failed" }
    & npm config set fetch-retry-maxtimeout 120000
    if ($LASTEXITCODE -ne 0) { throw "npm config failed" }

    $maxAttempts = 3
    for ($attempt = 1; $attempt -le $maxAttempts; $attempt++) {
        & npm ci --prefer-offline --no-audit
        if ($LASTEXITCODE -eq 0) { return }
        if ($attempt -eq $maxAttempts) { throw "npm ci failed" }
        Write-Warn "npm ci failed on attempt $attempt of $maxAttempts; retrying after transient npm/network failure."
        Start-Sleep -Seconds (15 * $attempt)
    }
}

function Sign-Binary([string]$FilePath) {
    if (-not (Test-Path $FilePath)) {
        Write-Warn "Skipping $FilePath (not found)"
        return
    }

    $fileName = Split-Path $FilePath -Leaf

    switch ($SignTool) {
        "pfx" {
            $cert = if ($CertFile) { $CertFile } else { if ($env:ZORAI_SIGN_CERT) { $env:ZORAI_SIGN_CERT } else { $env:ZORAI_SIGN_CERT } }
            $pass = if ($CertPassword) { $CertPassword } else { if ($env:ZORAI_SIGN_PASSWORD) { $env:ZORAI_SIGN_PASSWORD } else { $env:ZORAI_SIGN_PASSWORD } }
            if (-not $cert) {
                Write-Warn "No PFX certificate. Set -CertFile or ZORAI_SIGN_CERT."
                return
            }
            Write-Host "  Signing $fileName (PFX)..."
            & signtool sign /f $cert /p $pass /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $FilePath
            if ($LASTEXITCODE -ne 0) { throw "signtool failed for $fileName" }
        }
        "signtool" {
            $thumb = if ($Thumbprint) { $Thumbprint } else { if ($env:ZORAI_SIGN_THUMBPRINT) { $env:ZORAI_SIGN_THUMBPRINT } else { $env:ZORAI_SIGN_THUMBPRINT } }
            if (-not $thumb) {
                Write-Warn "No certificate thumbprint. Set -Thumbprint or ZORAI_SIGN_THUMBPRINT."
                return
            }
            Write-Host "  Signing $fileName (cert store)..."
            & signtool sign /sha1 $thumb /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $FilePath
            if ($LASTEXITCODE -ne 0) { throw "signtool failed for $fileName" }
        }
        "azure" {
            # Azure Trusted Signing via AzureSignTool
            # Requires: dotnet tool install -g AzureSignTool
            $endpoint  = $env:AZURE_SIGN_ENDPOINT
            $account   = $env:AZURE_SIGN_ACCOUNT
            $certProf  = $env:AZURE_SIGN_CERT_PROFILE
            $tenantId  = $env:AZURE_TENANT_ID
            $clientId  = $env:AZURE_CLIENT_ID
            $clientSec = $env:AZURE_CLIENT_SECRET

            if (-not ($endpoint -and $account -and $certProf)) {
                Write-Warn "Azure Trusted Signing not configured. Set AZURE_SIGN_* env vars."
                return
            }
            Write-Host "  Signing $fileName (Azure Trusted Signing)..."
            & AzureSignTool sign `
                -kvu $endpoint `
                -kva $account `
                -kvt $tenantId `
                -kvi $clientId `
                -kvs $clientSec `
                -kvc $certProf `
                -tr http://timestamp.digicert.com `
                -td SHA256 `
                $FilePath
            if ($LASTEXITCODE -ne 0) { throw "AzureSignTool failed for $fileName" }
        }
    }
    Write-Ok "Signed $fileName"
}

function Verify-Signature([string]$FilePath) {
    if (-not (Test-Path $FilePath)) { return }
    $sig = Get-AuthenticodeSignature $FilePath
    $fileName = Split-Path $FilePath -Leaf
    if ($sig.Status -eq "Valid") {
        Write-Ok "$fileName signature: Valid ($($sig.SignerCertificate.Subject))"
    } else {
        Write-Warn "$fileName signature: $($sig.Status)"
    }
}

function Get-FileSha256([string]$FilePath) {
    return (Get-FileHash -Path $FilePath -Algorithm SHA256).Hash.ToLower()
}

function Write-ChecksumsFile([string]$OutputPath, [string[]]$ArtifactNames) {
    $lines = foreach ($artifact in $ArtifactNames) {
        $hash = Get-FileSha256 (Join-Path $OutDir $artifact)
        "$hash  $artifact"
    }

    Set-Content -Path $OutputPath -Value $lines
}

function New-BundleZip([string]$ZipPath, [string[]]$ArtifactNames) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    if (Test-Path $ZipPath) {
        Remove-Item $ZipPath -Force
    }

    $archive = [System.IO.Compression.ZipFile]::Open($ZipPath, [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($artifact in $ArtifactNames) {
            [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                $archive,
                (Join-Path $OutDir $artifact),
                $artifact
            ) | Out-Null
        }

        $skillsRoot = Join-Path $ProjectRoot "skills"
        if (Test-Path $skillsRoot) {
            Get-ChildItem $skillsRoot -Recurse -File | ForEach-Object {
                $relative = [System.IO.Path]::GetRelativePath($ProjectRoot.Path, $_.FullName).Replace("\", "/")
                [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                    $archive,
                    $_.FullName,
                    $relative
                ) | Out-Null
            }
        }

        $guidelinesRoot = Join-Path $ProjectRoot "guidelines"
        if (Test-Path $guidelinesRoot) {
            Get-ChildItem $guidelinesRoot -Recurse -File | ForEach-Object {
                $relative = [System.IO.Path]::GetRelativePath($ProjectRoot.Path, $_.FullName).Replace("\", "/")
                [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                    $archive,
                    $_.FullName,
                    $relative
                ) | Out-Null
            }
        }

        $pluginsRoot = Join-Path $ProjectRoot "plugins"
        $skippedPluginDirs = @("node_modules", "__pycache__", ".pytest_cache", "tests", ".python", ".venv")
        if (Test-Path $pluginsRoot) {
            Get-ChildItem $pluginsRoot -Recurse -File | ForEach-Object {
                $relative = [System.IO.Path]::GetRelativePath($ProjectRoot.Path, $_.FullName).Replace("\", "/")
                $parts = $relative.Split("/")
                if (-not ($parts | Where-Object { $skippedPluginDirs -contains $_ })) {
                    [System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                        $archive,
                        $_.FullName,
                        $relative
                    ) | Out-Null
                }
            }
        }
    } finally {
        $archive.Dispose()
    }
}

# ─────────────────────────────────────────────────────
# Build steps
# ─────────────────────────────────────────────────────

$totalSteps = 6
$step = 0

Write-Host "`n============================================================" -ForegroundColor White
Write-Host " zorai release build" -ForegroundColor White
Write-Host "============================================================" -ForegroundColor White

# Setup preflight
Write-Step 0 $totalSteps "Running setup preflight..."
& (Join-Path $PSScriptRoot "setup.ps1") -Check -Profile source -Format text
if ($LASTEXITCODE -ne 0) { throw "Setup preflight failed" }
Write-Ok "Setup preflight complete"

# Step 1: Rust
$step++
if ($SkipRust) {
    Write-Step $step $totalSteps "Skipping Rust build (--SkipRust)"
} else {
    Write-Step $step $totalSteps "Building Rust binaries (release)..."
    Push-Location $ProjectRoot
    try {
        $cargoArgs = @("build", "--release")
        if ($Target) { $cargoArgs += @("--target", $Target) }
        & cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) { throw "Cargo build failed" }
        Write-Ok "Rust build complete"
    } finally {
        Pop-Location
    }
}

# Step 2: Frontend
$step++
if ($SkipFrontend) {
    Write-Step $step $totalSteps "Skipping frontend build (--SkipFrontend)"
} else {
    Write-Step $step $totalSteps "Building frontend..."
    Push-Location $FrontendDir
    try {
        Invoke-NpmCiWithRetries
        & npm run build
        if ($LASTEXITCODE -ne 0) { throw "Frontend build failed" }
        Write-Ok "Frontend build complete"
    } finally {
        Pop-Location
    }
}

# Step 3: Collect artifacts
$step++
Write-Step $step $totalSteps "Collecting artifacts..."
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Get-ChildItem $OutDir -Filter "zorai*" -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
Get-ChildItem $OutDir -Filter "zorai*" -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue

$targetDir = if ($Target) { Join-Path $ProjectRoot "target" $Target "release" } else { Join-Path $ProjectRoot "target" "release" }

$binaries = @(
    @{ Name = "zorai-daemon"; Desc = "Daemon" },
    @{ Name = "zorai";        Desc = "CLI" },
    @{ Name = "zoi";          Desc = "CLI alias" },
    @{ Name = "zorai-tui";    Desc = "TUI" },
    @{ Name = "zorai-mcp";    Desc = "MCP server" },
    @{ Name = "zorai-gateway"; Desc = "Chat gateway" }
)

foreach ($bin in $binaries) {
    $exe = Join-Path $targetDir "$($bin.Name).exe"
    if (Test-Path $exe) {
        Copy-Item $exe $OutDir -Force
        Write-Ok "$($bin.Desc): $($bin.Name).exe"
    } else {
        # Try without .exe (Linux build on WSL)
        $nix = Join-Path $targetDir $bin.Name
        if (Test-Path $nix) {
            Copy-Item $nix $OutDir -Force
            Write-Ok "$($bin.Desc): $($bin.Name)"
        }
    }
}

# Copy to frontend/dist for Electron bundling
$distDir = Join-Path $FrontendDir "dist"
if (Test-Path $distDir) {
    foreach ($name in @("zorai-daemon.exe", "zorai.exe", "zoi.exe", "zorai-tui.exe", "zorai-mcp.exe", "zorai-gateway.exe")) {
        $src = Join-Path $OutDir $name
        if (Test-Path $src) { Copy-Item $src $distDir -Force }
    }
}
$gettingStartedSrc = Join-Path $ProjectRoot "docs/getting-started.md"
if (Test-Path $gettingStartedSrc) {
    Copy-Item $gettingStartedSrc (Join-Path $OutDir "GETTING_STARTED.md") -Force
    if (Test-Path $distDir) {
        Copy-Item $gettingStartedSrc (Join-Path $distDir "GETTING_STARTED.md") -Force
    }
}

# Step 4: Code signing
$step++
if ($Sign) {
    Write-Step $step $totalSteps "Signing binaries..."
    Get-ChildItem $OutDir -Filter "*.exe" | ForEach-Object {
        Sign-Binary $_.FullName
    }
    Write-Ok "Signing complete"
} else {
    Write-Step $step $totalSteps "Skipping code signing (use -Sign to enable)"
}

# Step 5: Electron
$step++
if ($SkipElectron) {
    Write-Step $step $totalSteps "Skipping Electron build (--SkipElectron)"
} else {
    Write-Step $step $totalSteps "Building Electron app..."
    Push-Location $FrontendDir
    try {
        # Set signing env vars for electron-builder
        if ($Sign) {
            if ($CertFile -or $env:ZORAI_SIGN_CERT -or $env:ZORAI_SIGN_CERT) {
                $env:CSC_LINK = if ($CertFile) { $CertFile } else { if ($env:ZORAI_SIGN_CERT) { $env:ZORAI_SIGN_CERT } else { $env:ZORAI_SIGN_CERT } }
                $env:CSC_KEY_PASSWORD = if ($CertPassword) { $CertPassword } else { if ($env:ZORAI_SIGN_PASSWORD) { $env:ZORAI_SIGN_PASSWORD } else { $env:ZORAI_SIGN_PASSWORD } }
            }
            if ($Thumbprint -or $env:ZORAI_SIGN_THUMBPRINT -or $env:ZORAI_SIGN_THUMBPRINT) {
                # For electron-builder custom signing, we need a sign hook
                $env:ZORAI_SIGN_THUMBPRINT = if ($Thumbprint) { $Thumbprint } elseif ($env:ZORAI_SIGN_THUMBPRINT) { $env:ZORAI_SIGN_THUMBPRINT } else { $env:ZORAI_SIGN_THUMBPRINT }
            }
        }

        Get-ChildItem (Join-Path $FrontendDir "release") -Filter "zorai*" -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
        Get-ChildItem (Join-Path $FrontendDir "release") -Filter "zorai*" -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
        $electronArgs = @("electron-builder", "--win", "portable", "nsis")
        if ($Target -match "aarch64|arm64") {
            $electronArgs += "--arm64"
        }
        & npx @electronArgs
        if ($LASTEXITCODE -ne 0) { throw "Electron build failed" }

        # Copy Electron artifacts
        $releaseDir = Join-Path $FrontendDir "release"
        Get-ChildItem $releaseDir -Filter "zorai*.exe" -ErrorAction SilentlyContinue | ForEach-Object {
            Copy-Item $_.FullName $OutDir -Force
            Write-Ok "Electron: $($_.Name)"
        }
        $portable = Join-Path $releaseDir "zorai-portable.exe"
        if (Test-Path $portable) {
            Copy-Item $portable (Join-Path $OutDir "zorai-desktop.exe") -Force
            Write-Ok "Electron launcher: zorai-desktop.exe"
        }
    } finally {
        Pop-Location
    }
}

# ─────────────────────────────────────────────────────
# Verify signatures
# ─────────────────────────────────────────────────────
if ($Sign) {
    Write-Host "`nVerifying signatures..." -ForegroundColor Cyan
    Get-ChildItem $OutDir -Filter "*.exe" | ForEach-Object {
        Verify-Signature $_.FullName
    }
}

# Step 6: Package bundle + checksums
$step++
Write-Step $step $totalSteps "Packaging release bundle..."
$bundleArtifacts = Get-ChildItem $OutDir -File | Where-Object {
    $_.Name -notmatch '\.zip$' -and
    $_.Name -notmatch '^SHA256SUMS.*\.txt$' -and
    $_.Name -ne 'RELEASE_NOTES.md'
} | Select-Object -ExpandProperty Name

if ($bundleArtifacts.Count -gt 0) {
    $artifactArch = if ($Target -match "aarch64|arm64") { "arm64" } else { "x64" }
    $checksumsFile = Join-Path $OutDir "SHA256SUMS-windows-$artifactArch.txt"
    $bundleFile = Join-Path $OutDir "zorai-windows-$artifactArch.zip"
    $notesFile = Join-Path $OutDir "RELEASE_NOTES.md"

    if (-not (Test-Path $notesFile)) {
        @(
            "# zorai Windows Release Notes",
            "",
            "Built on $(Get-Date -AsUTC -Format 'yyyy-MM-dd HH:mm UTC').",
            "",
            "Bundled built-in skills and guidelines are included under the archive skills/ and guidelines/ trees."
        ) | Set-Content -Path $notesFile
    }

    Write-ChecksumsFile $checksumsFile $bundleArtifacts
    New-BundleZip $bundleFile ($bundleArtifacts + @((Split-Path $checksumsFile -Leaf), (Split-Path $notesFile -Leaf)))
    Write-Ok "Created $(Split-Path $checksumsFile -Leaf)"
    Write-Ok "Created $(Split-Path $notesFile -Leaf)"
    Write-Ok "Created $(Split-Path $bundleFile -Leaf)"
}

# ─────────────────────────────────────────────────────
# Summary
# ─────────────────────────────────────────────────────
Write-Host "`n============================================================" -ForegroundColor White
Write-Host " Build complete!" -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor White
Write-Host ""
Write-Host "  Output: $OutDir" -ForegroundColor White
Write-Host ""
Get-ChildItem $OutDir | ForEach-Object {
    $size = "{0:N1} MB" -f ($_.Length / 1MB)
    Write-Host ("  {0,-30} {1}" -f $_.Name, $size) -ForegroundColor Gray
}
Write-Host ""

if (-not $Sign) {
    Write-Host "  Binaries are NOT signed. Run with -Sign to sign." -ForegroundColor Yellow
}
Write-Host "============================================================" -ForegroundColor White
