#!/usr/bin/env pwsh
# BeeBotOS Development Manager (Windows)
# Usage: .\scripts\beebotos-dev.ps1 [menu|build|start|stop|restart|run|status] [service|all]

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Resolve-Path (Join-Path $ScriptDir "..")
$PidDir = Join-Path $ProjectRoot "data\run"
New-Item -ItemType Directory -Force -Path $PidDir | Out-Null

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
        BuildCmd = "cargo build --release -p beebotos-web --target wasm32-unknown-unknown; if (`$?) { cargo build --release -p beebotos-web --bin web-server }"
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
        $pid = Get-Content $pidFile -Raw
        $pid = $pid.Trim()
        try {
            $proc = Get-Process -Id $pid -ErrorAction SilentlyContinue
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

    if (-not $svc.BuildCmd) {
        Print-Warn "No build command for $name, skipping."
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
        $pid = (Get-Content $pidFile -Raw).Trim()
        Print-Warn "$name is already running (PID: $pid)"
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

    $logFile = Join-Path $PidDir "$name.log"
    $proc = Start-Process -FilePath $binaryPath -RedirectStandardOutput $logFile -RedirectStandardError $logFile -PassThru -WindowStyle Hidden
    $proc.Id | Set-Content $pidFile -NoNewline

    Start-Sleep -Seconds 1
    try {
        $check = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
        if ($check) {
            Print-Success "$name started (PID: $($proc.Id))"
            return $true
        }
    } catch {}

    Print-Error "$name failed to start. Check $logFile"
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

    $pid = (Get-Content $pidFile -Raw).Trim()
    Write-Host "Stopping $name (PID: $pid)..." -ForegroundColor Cyan

    try {
        Stop-Process -Id $pid -Force -ErrorAction Stop
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
            $pid = (Get-Content $pidFile -Raw).Trim()
            $line = "{0,-12} {1,-10} {2,-8} {3}" -f $svc.Name, "running", $pid, $svc.Port
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
        "status" { Show-Status }
        default {
            Print-Error "Unknown action: $action"
            Write-Host "Usage: beebotos-dev.ps1 [menu|build|start|stop|restart|run|status] [service|all]"
            Write-Host ""
            Write-Host "Actions:"
            Write-Host "  build    - Compile a service"
            Write-Host "  start    - Start a service"
            Write-Host "  stop     - Stop a service"
            Write-Host "  restart  - Restart a service"
            Write-Host "  run      - Build and start a service"
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
