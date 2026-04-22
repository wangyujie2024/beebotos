#!/usr/bin/env pwsh
# BeeBotOS Development Manager (Windows)
# Usage: .\scripts\beebotos-dev.ps1 [menu|build|start|stop|restart|run|pack|status] [service|all]

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Resolve-Path (Join-Path $ScriptDir "..")
$PidDir = Join-Path $ProjectRoot "data\run"
$LogDir = Join-Path $ProjectRoot "data\logs"
New-Item -ItemType Directory -Force -Path $PidDir | Out-Null
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

Set-Location $ProjectRoot

function Print-Header {
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host "  BeeBotOS Development Manager" -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
    Write-Host ""
}

function Print-Error($msg)   { Write-Host "[ERROR] $msg" -ForegroundColor Red }
function Print-Info($msg)    { Write-Host "[INFO] $msg" -ForegroundColor Blue }
function Print-Success($msg) { Write-Host "[OK] $msg" -ForegroundColor Green }
function Print-Warn($msg)    { Write-Host "[WARN] $msg" -ForegroundColor Yellow }

# Service definitions
$Services = @(
    @{
        Name = "gateway"
        BuildCmd = "cargo build --release -p beebotos-gateway"
        Binary = "target\release\beebotos-gateway.exe"
        Port = 8000
        Desc = "API Gateway"
    },
    @{
        Name = "web"
        BuildCmd = $null  # handled specially in Build-Service
        Binary = "target\release\web-server.exe"
        Port = 8090
        Desc = "Web Frontend Server"
    },
    @{
        Name = "beehub"
        BuildCmd = "cargo build --release -p beebotos-beehub"
        Binary = "target\release\beehub.exe"
        Port = 8080
        Desc = "BeeHub Service"
    },
    @{
        Name = "cli"
        BuildCmd = "cargo install --path apps\cli --force"
        Binary = $null
        Port = 0
        Desc = "CLI Tool (install only)"
    }
)

function Get-Service($name) {
    foreach ($svc in $Services) {
        if ($svc.Name -eq $name) { return $svc }
    }
    return $null
}

function Get-ServiceNames() {
    return $Services | ForEach-Object { $_.Name }
}

function Get-PidFile($name) {
    return Join-Path $PidDir "$name.pid"
}

function Test-IsRunning($name) {
    $pidFile = Get-PidFile $name
    if (Test-Path $pidFile) {
        $procId = Get-Content $pidFile -Raw
        $procId = $procId.Trim()
        try {
            $proc = Get-Process -Id $procId -ErrorAction SilentlyContinue
            if ($proc) { return $true }
        } catch {}
    }
    return $false
}

function Build-Service($name) {
    $svc = Get-Service $name
    if (-not $svc) { Print-Error "Unknown service: $name"; return $false }

    Write-Host "----------------------------------------" -ForegroundColor Cyan
    Write-Host "Building: $($svc.Desc) ($name)" -ForegroundColor Cyan
    Write-Host "----------------------------------------" -ForegroundColor Cyan

    if (-not $svc.BuildCmd -and $name -ne "web") {
        Print-Warn "No build command for $name, skipping."
        return $true
    }

    # Check for cargo
    try {
        $null = Get-Command cargo -ErrorAction Stop
    } catch {
        Print-Error "cargo not found in PATH. Please install Rust: https://rustup.rs"
        return $false
    }

    # Special handling for web service which has multi-step build
    if ($name -eq "web") {
        try {
            $null = Get-Command wasm-pack -ErrorAction Stop
        } catch {
            Print-Error "wasm-pack not found in PATH. Please install it: cargo install wasm-pack"
            return $false
        }

        cargo build --release --lib -p beebotos-web --target wasm32-unknown-unknown
        if ($LASTEXITCODE -ne 0) {
            Print-Error "Build failed: web - cargo build lib failed (exit $LASTEXITCODE)"
            return $false
        }
        wasm-pack build --target web --out-dir pkg apps/web/
        if ($LASTEXITCODE -ne 0) {
            Print-Error "Build failed: web - wasm-pack build failed (exit $LASTEXITCODE)"
            return $false
        }
        cargo build --release --bin web-server
        if ($LASTEXITCODE -ne 0) {
            Print-Error "Build failed: web - cargo build web-server failed (exit $LASTEXITCODE)"
            return $false
        }
        Print-Success "Build completed: $name"
        return $true
    }

    try {
        Invoke-Expression $svc.BuildCmd
        if ($LASTEXITCODE -eq 0 -or $null -eq $LASTEXITCODE) {
            Print-Success "Build completed: $name"
            return $true
        } else {
            Print-Error "Build failed: $name (exit $LASTEXITCODE)"
            return $false
        }
    } catch {
        Print-Error "Build failed: $name - $($_.Exception.Message)"
        return $false
    }
}

function Start-Service($name) {
    $svc = Get-Service $name
    if (-not $svc) { Print-Error "Unknown service: $name"; return $false }

    if (-not $svc.Binary) {
        Print-Warn "$name is not a daemon service, skipping start."
        return $true
    }

    $pidFile = Get-PidFile $name
    if (Test-IsRunning $name) {
        $procId = (Get-Content $pidFile -Raw).Trim()
        Print-Warn "$name is already running (PID: $procId)"
        return $true
    }

    $binaryPath = Join-Path $ProjectRoot $svc.Binary
    if (-not (Test-Path $binaryPath)) {
        Print-Error "Binary not found: $binaryPath"
        Print-Info "Please build $name first."
        return $false
    }

    Write-Host "Starting: $($svc.Desc) ($name)" -ForegroundColor Cyan
    Print-Info "Binary: $binaryPath"
    Print-Info "Port: $($svc.Port)"

    $outFile = Join-Path $LogDir "$name.log"
    $errFile = Join-Path $LogDir "$name.err"

    # web-server needs correct static-path and gateway-url to work properly
    $startArgs = @{}
    if ($name -eq "web") {
        # 准备临时静态目录，解决 CSS/favicon 占位符问题
        $tempStaticDir = Join-Path $ProjectRoot "data\temp-web-static"
        if (Test-Path $tempStaticDir) { Remove-Item -Recurse -Force $tempStaticDir }
        New-Item -ItemType Directory -Force -Path $tempStaticDir | Out-Null
        Copy-Item (Join-Path $ProjectRoot "apps\web\index.html") $tempStaticDir
        Copy-Item -Recurse (Join-Path $ProjectRoot "apps\web\pkg") $tempStaticDir
        Copy-Item -Recurse (Join-Path $ProjectRoot "apps\web\style") $tempStaticDir
        Copy-Item (Join-Path $ProjectRoot "apps\web\style\main.css") (Join-Path $tempStaticDir "style.css")
        Copy-Item (Join-Path $ProjectRoot "apps\web\style\components.css") (Join-Path $tempStaticDir "components.css")
        $realFavicon = Join-Path $ProjectRoot "apps\web\public\favicon.svg"
        if (Test-Path $realFavicon) {
            Copy-Item $realFavicon (Join-Path $tempStaticDir "favicon.svg")
        }
        $startArgs["ArgumentList"] = "`"--static-path`" `"$tempStaticDir`" `"--gateway-url`" http://localhost:8000"
        Print-Info "Static path: $tempStaticDir"
        Print-Info "Gateway URL: http://localhost:8000"
    }

    $proc = Start-Process -FilePath $binaryPath @startArgs -RedirectStandardOutput $outFile -RedirectStandardError $errFile -PassThru -WindowStyle Hidden
    $proc.Id | Set-Content $pidFile -NoNewline

    Start-Sleep -Seconds 1
    try {
        $check = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
        if ($check) {
            Print-Success "$name started (PID: $($proc.Id))"
            return $true
        }
    } catch {}

    Print-Error "$name failed to start. Check $outFile"
    Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
    return $false
}

function Stop-Service($name) {
    $pidFile = Get-PidFile $name
    if (-not (Test-IsRunning $name)) {
        Print-Warn "$name is not running"
        Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
        return
    }

    $procId = (Get-Content $pidFile -Raw).Trim()
    Write-Host "Stopping $name (PID: $procId)..." -ForegroundColor Cyan

    try {
        Stop-Process -Id $procId -Force -ErrorAction Stop
        Print-Success "$name stopped"
    } catch {
        Print-Warn "Could not stop $name gracefully: $($_.Exception.Message)"
    }
    Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
}

function Restart-Service($name) {
    Stop-Service $name
    Start-Sleep -Seconds 1
    Start-Service $name | Out-Null
}

function Build-And-Start($name) {
    if (Build-Service $name) {
        Start-Service $name | Out-Null
    }
}

function Pack-Release($target = "all") {
    Write-Host "----------------------------------------" -ForegroundColor Cyan
    Write-Host "Packing release for target: $target" -ForegroundColor Cyan
    Write-Host "----------------------------------------" -ForegroundColor Cyan

    $outDir = Join-Path $ProjectRoot "dist\beebotos"
    $archive = Join-Path $ProjectRoot "dist\beebotos-x64-pc-windows-msvc.zip"

    if (Test-Path $outDir) { Remove-Item -Recurse -Force $outDir }
    New-Item -ItemType Directory -Force -Path $outDir | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $outDir "pkg") | Out-Null

    if ($target -eq "all" -or $target -eq "gateway") {
        Copy-Item (Join-Path $ProjectRoot "target\release\beebotos-gateway.exe") $outDir
        Copy-Item -Recurse (Join-Path $ProjectRoot "migrations_sqlite") $outDir
    }
    if ($target -eq "all" -or $target -eq "web") {
        Copy-Item (Join-Path $ProjectRoot "target\release\web-server.exe") $outDir
        $pkgSource = Join-Path $ProjectRoot "apps\web\pkg"
        $pkgDest = Join-Path $outDir "pkg"
        if (-not (Test-Path $pkgSource)) {
            Print-Error "Web pkg directory not found: $pkgSource"
            Print-Info "Please build the web service first: .\scripts\beebotos-dev.ps1 build web"
            Remove-Item -Recurse -Force $outDir -ErrorAction SilentlyContinue
            exit 1
        }
        Get-ChildItem -Path $pkgSource | ForEach-Object {
            Copy-Item -Path $_.FullName -Destination $pkgDest -Recurse -Force
        }
        # Copy static web assets (favicon.svg 在 apps/web/ 下是占位符，从 public/ 复制真实文件)
        $src = Join-Path $ProjectRoot "apps\web\index.html"
        if (Test-Path $src) {
            Copy-Item $src $outDir
        }
        $realFavicon = Join-Path $ProjectRoot "apps\web\public\favicon.svg"
        if (Test-Path $realFavicon) {
            Copy-Item $realFavicon (Join-Path $outDir "favicon.svg")
        }
        # Copy real CSS from style/ directory (root CSS files are redirects)
        $styleDir = Join-Path $ProjectRoot "apps\web\style"
        if (Test-Path $styleDir) {
            Copy-Item -Recurse $styleDir $outDir
            # Copy actual CSS files to root for index.html references
            Copy-Item (Join-Path $styleDir "main.css") (Join-Path $outDir "style.css")
            Copy-Item (Join-Path $styleDir "components.css") (Join-Path $outDir "components.css")
        }
    }
    if ($target -eq "all" -or $target -eq "beehub") {
        $beehubPath = Join-Path $ProjectRoot "target\release\beehub.exe"
        if (Test-Path $beehubPath) {
            Copy-Item $beehubPath $outDir
        } else {
            Print-Warn "beehub.exe not found, skipping"
        }
    }

    if (Test-Path (Join-Path $ProjectRoot "config")) {
        Copy-Item -Recurse (Join-Path $ProjectRoot "config") $outDir
        # 调整 web-server 生产配置：静态文件路径指向当前目录
        $prodConfig = Join-Path $outDir "config\web-server.toml"
        if (Test-Path $prodConfig) {
            (Get-Content $prodConfig) -replace 'path = "apps/web"', 'path = "."' | Set-Content $prodConfig -Encoding UTF8
        }
    }

    Copy-Item (Join-Path $ProjectRoot "scripts\beebotos-run.ps1") $outDir

    Compress-Archive -Path $outDir -DestinationPath $archive -Force
    Print-Success "Release packed: $archive"
    Write-Host "Contents:"
    Get-ChildItem $outDir | Format-Table Name, @{Label="Size"; Expression={$_.Length}; Align="Right"}
}

function Show-Status {
    Write-Host "Service Status" -ForegroundColor Cyan
    Write-Host "----------------------------------------" -ForegroundColor Cyan
    Write-Host ("{0,-12} {1,-10} {2,-8} {3}" -f "Service", "Status", "PID", "Port")
    Write-Host "----------------------------------------"
    foreach ($svc in $Services) {
        if (-not $svc.Binary) {
            Write-Host ("{0,-12} {1,-10} {2,-8} {3}" -f $svc.Name, "N/A", "-", "install-only")
            continue
        }
        $pidFile = Get-PidFile $svc.Name
        if (Test-IsRunning $svc.Name) {
            $procId = (Get-Content $pidFile -Raw).Trim()
            $line = "{0,-12} {1,-10} {2,-8} {3}" -f $svc.Name, "running", $procId, $svc.Port
            Write-Host $line -ForegroundColor Green
        } else {
            $line = "{0,-12} {1,-10} {2,-8} {3}" -f $svc.Name, "stopped", "-", $svc.Port
            Write-Host $line -ForegroundColor Red
        }
    }
}

function Show-Menu {
    Clear-Host
    Print-Header
    Write-Host "  1) Build"
    Write-Host "     1.1) Build Gateway"
    Write-Host "     1.2) Build Web"
    Write-Host "     1.3) Build CLI"
    Write-Host "     1.4) Build BeeHub"
    Write-Host "     1.5) Build All"
    Write-Host ""
    Write-Host "  2) Start"
    Write-Host "     2.1) Start Gateway"
    Write-Host "     2.2) Start Web"
    Write-Host "     2.3) Start BeeHub"
    Write-Host "     2.4) Start All"
    Write-Host ""
    Write-Host "  3) Stop"
    Write-Host "     3.1) Stop Gateway"
    Write-Host "     3.2) Stop Web"
    Write-Host "     3.3) Stop BeeHub"
    Write-Host "     3.4) Stop All"
    Write-Host ""
    Write-Host "  4) Restart"
    Write-Host "     4.1) Restart Gateway"
    Write-Host "     4.2) Restart Web"
    Write-Host "     4.3) Restart BeeHub"
    Write-Host "     4.4) Restart All"
    Write-Host ""
    Write-Host "  5) Build & Start"
    Write-Host "     5.1) Build & Start Gateway"
    Write-Host "     5.2) Build & Start Web"
    Write-Host "     5.3) Build & Start BeeHub"
    Write-Host "     5.4) Build & Start All"
    Write-Host ""
    Write-Host "  6) Status"
    Write-Host "  7) Pack Release"
    Write-Host "  0) Exit"
    Write-Host ""
    $choice = Read-Host "Select option"
    return $choice
}

function Handle-Menu {
    while ($true) {
        $choice = Show-Menu
        Write-Host ""

        switch ($choice) {
            { $_ -in "1", "1.1" } { Build-Service "gateway" }
            "1.2" { Build-Service "web" }
            "1.3" { Build-Service "cli" }
            "1.4" { Build-Service "beehub" }
            "1.5" {
                foreach ($svc in @("gateway", "web", "cli", "beehub")) {
                    Build-Service $svc | Out-Null
                }
            }
            { $_ -in "2", "2.1" } { Start-Service "gateway" | Out-Null }
            "2.2" { Start-Service "web" | Out-Null }
            "2.3" { Start-Service "beehub" | Out-Null }
            "2.4" {
                foreach ($svc in @("gateway", "web", "beehub")) {
                    Start-Service $svc | Out-Null
                }
            }
            { $_ -in "3", "3.1" } { Stop-Service "gateway" }
            "3.2" { Stop-Service "web" }
            "3.3" { Stop-Service "beehub" }
            "3.4" {
                foreach ($svc in @("gateway", "web", "beehub")) {
                    Stop-Service $svc
                }
            }
            { $_ -in "4", "4.1" } { Restart-Service "gateway" }
            "4.2" { Restart-Service "web" }
            "4.3" { Restart-Service "beehub" }
            "4.4" {
                foreach ($svc in @("gateway", "web", "beehub")) {
                    Restart-Service $svc
                }
            }
            { $_ -in "5", "5.1" } { Build-And-Start "gateway" }
            "5.2" { Build-And-Start "web" }
            "5.3" { Build-And-Start "beehub" }
            "5.4" {
                foreach ($svc in @("gateway", "web", "beehub")) {
                    Build-And-Start $svc
                }
            }
            "6" { Show-Status }
            "7" { Pack-Release "all" }
            { $_ -in "0", "q", "quit", "exit" } { Write-Host "Goodbye!"; exit 0 }
            default { Print-Warn "Invalid option: $choice" }
        }

        Write-Host ""
        Read-Host "Press Enter to continue"
    }
}

function Handle-Cli($action, $target = "all") {
    $validServices = Get-ServiceNames
    if ($target -ne "all" -and $target -notin $validServices) {
        Print-Error "Unknown service: $target"
        Print-Info "Available: $($validServices -join ' ') all"
        exit 1
    }

    switch ($action) {
        "build" {
            $list = if ($target -eq "all") { @("gateway", "web", "cli", "beehub") } else { @($target) }
            foreach ($svc in $list) { Build-Service $svc | Out-Null }
        }
        "start" {
            $list = if ($target -eq "all") { @("gateway", "web", "beehub") } else { @($target) }
            foreach ($svc in $list) { Start-Service $svc | Out-Null }
        }
        "stop" {
            $list = if ($target -eq "all") { @("gateway", "web", "beehub") } else { @($target) }
            foreach ($svc in $list) { Stop-Service $svc }
        }
        "restart" {
            $list = if ($target -eq "all") { @("gateway", "web", "beehub") } else { @($target) }
            foreach ($svc in $list) { Restart-Service $svc }
        }
        "run" {
            $list = if ($target -eq "all") { @("gateway", "web", "beehub") } else { @($target) }
            foreach ($svc in $list) { Build-And-Start $svc }
        }
        "pack" { Pack-Release $target }
        "status" { Show-Status }
        default {
            Print-Error "Unknown action: $action"
            Write-Host "Usage: beebotos-dev.ps1 [menu|build|start|stop|restart|run|pack|status] [service|all]"
            Write-Host ""
            Write-Host "Actions:"
            Write-Host "  build    - Compile a service"
            Write-Host "  start    - Start a service"
            Write-Host "  stop     - Stop a service"
            Write-Host "  restart  - Restart a service"
            Write-Host "  run      - Build and start a service"
            Write-Host "  pack     - Package binaries and assets for deployment"
            Write-Host "  status   - Show service status"
            Write-Host "  menu     - Interactive menu (default)"
            Write-Host ""
            Write-Host "Services: $($validServices -join ' ') all"
            exit 1
        }
    }
}

$action = if ($args.Count -gt 0) { $args[0] } else { "menu" }

if ($action -eq "menu") {
    Handle-Menu
} else {
    $target = if ($args.Count -gt 1) { $args[1] } else { "all" }
    Handle-Cli $action $target
}
