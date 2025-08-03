# Cross-platform build script for comfy-fs (PowerShell)
param(
    [Parameter(Position=0)]
    [string]$Command = "build"
)

$ProjectName = "comfy-fs"
$BuildDir = "target/release"
$DistDir = "dist"

# Color functions
function Write-Log { param($Message) Write-Host "[BUILD] $Message" -ForegroundColor Blue }
function Write-Success { param($Message) Write-Host "[SUCCESS] $Message" -ForegroundColor Green }
function Write-Warn { param($Message) Write-Host "[WARN] $Message" -ForegroundColor Yellow }
function Write-Error { param($Message) Write-Host "[ERROR] $Message" -ForegroundColor Red }

function Clean {
    Write-Log "Cleaning previous builds..."
    if (Test-Path $DistDir) { Remove-Item -Recurse -Force $DistDir }
    New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
    cargo clean
}

function Test-Target {
    param($Target)
    $InstalledTargets = rustup target list --installed
    if ($InstalledTargets -notmatch $Target) {
        Write-Log "Installing target: $Target"
        rustup target add $Target
    }
}

function Build-Target {
    param($Target, $OutputName)
    
    Write-Log "Building for target: $Target"
    Test-Target $Target
    
    # Build with optimizations
    $env:RUSTFLAGS = "-C target-cpu=native"
    cargo build --release --target $Target --bin $ProjectName
    
    # Copy binary to dist directory
    $BinaryPath = "target/$Target/release/$ProjectName"
    if ($Target -match "windows") {
        $BinaryPath += ".exe"
        $OutputName += ".exe"
    }
    
    if (Test-Path $BinaryPath) {
        Copy-Item $BinaryPath "$DistDir/$OutputName"
        Write-Success "Built: $OutputName"
    } else {
        Write-Error "Failed to build for $Target"
        return $false
    }
    return $true
}

function Build-All {
    Write-Log "Starting cross-platform build..."
    
    # Windows targets
    Build-Target "x86_64-pc-windows-msvc" "comfy-fs-windows-x86_64"
    Build-Target "aarch64-pc-windows-msvc" "comfy-fs-windows-arm64"
    
    # Linux targets (if cross-compilation is set up)
    if (Get-Command "x86_64-unknown-linux-gnu-gcc" -ErrorAction SilentlyContinue) {
        Build-Target "x86_64-unknown-linux-gnu" "comfy-fs-linux-x86_64"
    } else {
        Write-Warn "Linux cross-compilation not available"
    }
}

function New-Checksums {
    Write-Log "Creating checksums..."
    Push-Location $DistDir
    Get-ChildItem -File | ForEach-Object {
        $Hash = Get-FileHash $_.Name -Algorithm SHA256
        "$($Hash.Hash.ToLower())  $($_.Name)"
    } | Out-File -Encoding ASCII "checksums.sha256"
    Pop-Location
    Write-Success "Checksums created"
}

function New-Packages {
    Write-Log "Creating packages..."
    Push-Location $DistDir
    
    Get-ChildItem "comfy-fs-*" -File | ForEach-Object {
        if ($_.Name -ne "checksums.sha256") {
            Compress-Archive -Path $_.Name -DestinationPath "$($_.BaseName).zip" -Force
            Write-Log "Packaged: $($_.BaseName).zip"
        }
    }
    
    Pop-Location
    Write-Success "All packages created"
}

function Show-Usage {
    Write-Host "Usage: .\build.ps1 [command]"
    Write-Host ""
    Write-Host "Commands:"
    Write-Host "  clean     Clean previous builds"
    Write-Host "  build     Build for current platform only"
    Write-Host "  all       Build for all platforms"
    Write-Host "  package   Package all builds"
    Write-Host "  help      Show this help"
    Write-Host ""
    Write-Host "Example: .\build.ps1 all"
}

# Main execution
switch ($Command.ToLower()) {
    "clean" {
        Clean
    }
    "build" {
        Clean
        $CurrentTarget = rustc -vV | Select-String "host:" | ForEach-Object { $_.ToString().Split()[1] }
        Build-Target $CurrentTarget $ProjectName
    }
    "all" {
        Clean
        Build-All
        New-Checksums
    }
    "package" {
        New-Packages
    }
    "help" {
        Show-Usage
    }
    default {
        Write-Error "Unknown command: $Command"
        Show-Usage
        exit 1
    }
}