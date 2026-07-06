<#
.SYNOPSIS
    Geese 引擎统一构建打包脚本
.DESCRIPTION
    构建 desktop、server、client 三个 Cargo 项目，将产物收集到 dist/ 目录，
    生成可直接运行的可执行文件包。
.PARAMETER Profile
    构建模式: debug 或 release（默认 release）
.PARAMETER Target
    构建目标: all、desktop、server、client（默认 all）
.EXAMPLE
    .\build.ps1
    .\build.ps1 -Profile debug
    .\build.ps1 -Target server
#>
param(
    [string]$Profile = "release",
    [string]$Target = "all"
)

$ErrorActionPreference = "Stop"
$Root = $PSScriptRoot

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Geese Engine Build Script" -ForegroundColor Cyan
Write-Host "  Profile: $Profile | Target: $Target" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# ── 1. 检测 Python 版本 ──
$PyVersion = & python -c "import sys; print(f'{sys.version_info.major}{sys.version_info.minor}')"
$PyTag = "cp$PyVersion-win_amd64"
Write-Host "[INFO] Python version: $PyVersion (tag: $PyTag)" -ForegroundColor Green

# ── 2. 构建函数 ──
function Build-Project {
    param([string]$Name, [string]$Path)
    Write-Host "[BUILD] Building $Name..." -ForegroundColor Yellow
    Push-Location (Join-Path $Root $Path)
    try {
        if ($Profile -eq "release") {
            & cargo build --release
        } else {
            & cargo build
        }
        if ($LASTEXITCODE -ne 0) {
            Write-Host "[ERROR] $Name build failed!" -ForegroundColor Red
            exit 1
        }
        Write-Host "[BUILD] $Name built successfully." -ForegroundColor Green
    }
    finally {
        Pop-Location
    }
}

if ($Target -in @("all", "desktop")) { Build-Project "desktop" "desktop" }
if ($Target -in @("all", "server"))  { Build-Project "server"  "server" }
if ($Target -in @("all", "client"))  { Build-Project "client"  "client" }

# ── 3. 准备 dist/ 目录 ──
$Dist = Join-Path $Root "dist"
$TargetDir = if ($Profile -eq "release") { "release" } else { "debug" }

Write-Host "[PACK] Preparing dist/ directory..." -ForegroundColor Yellow
if (Test-Path $Dist) { Remove-Item $Dist -Recurse -Force }
New-Item -ItemType Directory -Path $Dist -Force | Out-Null

# ── 4. 打包 desktop ──
if ($Target -in @("all", "desktop")) {
    Write-Host "[PACK] Packing desktop..." -ForegroundColor Yellow
    $DesktopDist = Join-Path $Dist "desktop"
    New-Item -ItemType Directory -Path $DesktopDist -Force | Out-Null

    # 复制并重命名 pydesktop.dll -> pydesktop.pyd
    $SrcDll = Join-Path $Root "desktop\target\$TargetDir\pydesktop.dll"
    if (Test-Path $SrcDll) {
        Copy-Item $SrcDll (Join-Path $DesktopDist "pydesktop.pyd") -Force
        Write-Host "  -> pydesktop.pyd" -ForegroundColor Gray
    } else {
        Write-Host "[WARN] pydesktop.dll not found at $SrcDll" -ForegroundColor Red
    }

    # 生成 desktop/run.py
    $RunPy = @"
import sys, os, traceback
import importlib.machinery, importlib.util

_dir = os.path.dirname(os.path.abspath(__file__))

# Load pydesktop extension
_dll = os.path.join(_dir, 'pydesktop.pyd')
_loader = importlib.machinery.ExtensionFileLoader('pydesktop', _dll)
_spec = importlib.util.spec_from_loader('pydesktop', _loader, origin=_dll)
pydesktop = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pydesktop)

project = sys.argv[1] if len(sys.argv) > 1 else os.path.join(_dir, 'projects', 'My')

try:
    pydesktop.open_editor(project)
    print('Editor exited normally', flush=True)
except SystemExit as e:
    print(f'SystemExit: {e.code}', flush=True)
except BaseException as e:
    with open(os.path.join(_dir, 'editor_crash.txt'), 'w') as f:
        f.write(f'{type(e).__name__}: {e}\n')
        traceback.print_exc(file=f)
    print(f'ERROR: {type(e).__name__}: {e}', flush=True)
"@
    Set-Content -Path (Join-Path $DesktopDist "run.py") -Value $RunPy -Encoding UTF8

    # 生成 desktop/run_launcher.py（启动 Launcher 模式）
    $LauncherPy = @"
import sys, os, traceback
import importlib.machinery, importlib.util

_dir = os.path.dirname(os.path.abspath(__file__))

_dll = os.path.join(_dir, 'pydesktop.pyd')
_loader = importlib.machinery.ExtensionFileLoader('pydesktop', _dll)
_spec = importlib.util.spec_from_loader('pydesktop', _loader, origin=_dll)
pydesktop = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pydesktop)

try:
    pydesktop.run()
    print('Launcher exited normally', flush=True)
except SystemExit as e:
    print(f'SystemExit: {e.code}', flush=True)
except BaseException as e:
    with open(os.path.join(_dir, 'launcher_crash.txt'), 'w') as f:
        f.write(f'{type(e).__name__}: {e}\n')
        traceback.print_exc(file=f)
    print(f'ERROR: {type(e).__name__}: {e}', flush=True)
"@
    Set-Content -Path (Join-Path $DesktopDist "run_launcher.py") -Value $LauncherPy -Encoding UTF8

    # 复制示例项目
    $ProjectSrc = Join-Path $Root "projects\My"
    if (Test-Path $ProjectSrc) {
        $ProjectDst = Join-Path $DesktopDist "projects\My"
        New-Item -ItemType Directory -Path (Join-Path $DesktopDist "projects") -Force | Out-Null
        Copy-Item $ProjectSrc $ProjectDst -Recurse -Force
        Write-Host "  -> projects/My" -ForegroundColor Gray
    }

    Write-Host "[PACK] Desktop packed." -ForegroundColor Green
}

# ── 5. 打包 server ──
if ($Target -in @("all", "server")) {
    Write-Host "[PACK] Packing server..." -ForegroundColor Yellow
    $ServerDist = Join-Path $Dist "server"
    New-Item -ItemType Directory -Path $ServerDist -Force | Out-Null

    # bin/ - 复制 exe 文件
    $BinDist = Join-Path $ServerDist "bin"
    New-Item -ItemType Directory -Path $BinDist -Force | Out-Null
    $ServerTarget = Join-Path $Root "server\target\$TargetDir"
    foreach ($exe in @("dbproxy.exe", "gate.exe")) {
        $SrcExe = Join-Path $ServerTarget $exe
        if (Test-Path $SrcExe) {
            Copy-Item $SrcExe (Join-Path $BinDist $exe) -Force
            Write-Host "  -> bin/$exe" -ForegroundColor Gray
        } else {
            Write-Host "[WARN] $exe not found at $SrcExe" -ForegroundColor Red
        }
    }

    # engine/ - 复制 Python 引擎框架
    $EngineSrc = Join-Path $Root "server\engine"
    $EngineDist = Join-Path $ServerDist "engine"
    if (Test-Path $EngineSrc) {
        Copy-Item $EngineSrc $EngineDist -Recurse -Force
        Write-Host "  -> engine/" -ForegroundColor Gray

        # 替换 pyhub 原生模块: 删除旧的 .pyd，放入新编译的
        $PyhubDir = Join-Path $EngineDist "pyhub"
        if (Test-Path $PyhubDir) {
            # 删除旧的不兼容 .pyd 文件
            Get-ChildItem $PyhubDir -Filter "*.pyd" | Remove-Item -Force
            $SrcPyhub = Join-Path $ServerTarget "pyhub.dll"
            if (Test-Path $SrcPyhub) {
                Copy-Item $SrcPyhub (Join-Path $PyhubDir "pyhub.pyd") -Force
                Write-Host "  -> engine/pyhub/pyhub.pyd" -ForegroundColor Gray
            } else {
                Write-Host "[WARN] pyhub.dll not found at $SrcPyhub" -ForegroundColor Red
            }
        }
    }

    # config/ - 复制配置文件
    $ConfigSrc = Join-Path $Root "sample\server\config"
    $ConfigDist = Join-Path $ServerDist "config"
    if (Test-Path $ConfigSrc) {
        Copy-Item $ConfigSrc $ConfigDist -Recurse -Force
        Write-Host "  -> config/" -ForegroundColor Gray

        # 修改 config 中的 log_dir 为绝对路径格式
        Get-ChildItem $ConfigDist -Filter "*.cfg" | ForEach-Object {
            $content = Get-Content $_.FullName -Raw
            $content = $content -replace '"../log"', '"./log"'
            Set-Content $_.FullName -Value $content -Encoding UTF8
        }
    }

    # src/ - 复制 Python 应用脚本
    $SrcAppDir = Join-Path $Root "sample\server\src"
    $SrcDist = Join-Path $ServerDist "src"
    if (Test-Path $SrcAppDir) {
        Copy-Item $SrcAppDir $SrcDist -Recurse -Force
        Write-Host "  -> src/" -ForegroundColor Gray
    }

    # dependences/ - 复制 Consul 和 Redis
    $DepSrc = Join-Path $Root "server\dependences"
    $DepDist = Join-Path $ServerDist "dependences"
    if (Test-Path $DepSrc) {
        Copy-Item $DepSrc $DepDist -Recurse -Force
        Write-Host "  -> dependences/ (consul + redis)" -ForegroundColor Gray
    }

    # 生成 server/start.ps1
    $StartPs1 = @"
`$Dir = `$PSScriptRoot
Write-Host "Starting Geese Server..." -ForegroundColor Cyan

# 1. Start Consul
Write-Host "[1/5] Starting Consul..." -ForegroundColor Yellow
Start-Process "`$Dir\dependences\consul\consul.exe" -ArgumentList "agent","-dev"

# 2. Start Redis
Write-Host "[2/5] Starting Redis..." -ForegroundColor Yellow
Start-Process "`$Dir\dependences\redis\redis-server.exe" -WorkingDirectory "`$Dir\dependences\redis"

# 3. Wait for services
Write-Host "[3/5] Waiting for Consul and Redis..." -ForegroundColor Yellow
Start-Sleep -Seconds 3

# 4. Start dbproxy and gate
Write-Host "[4/5] Starting dbproxy and gate..." -ForegroundColor Yellow
Start-Process "`$Dir\bin\dbproxy.exe" -ArgumentList "`$Dir\config\dbproxy.cfg" -WorkingDirectory "`$Dir\bin"
Start-Process "`$Dir\bin\gate.exe" -ArgumentList "`$Dir\config\gate.cfg" -WorkingDirectory "`$Dir\bin"

# 5. Wait for gate, then start hub scripts
Write-Host "[5/5] Starting hub (Python)..." -ForegroundColor Yellow
Start-Sleep -Seconds 3
Start-Process python -ArgumentList "`$Dir\src\app.py","`$Dir\config\player.cfg" -WorkingDirectory "`$Dir\src"
Start-Sleep -Seconds 2
Start-Process python -ArgumentList "`$Dir\src\rank_app.py","`$Dir\config\rank.cfg" -WorkingDirectory "`$Dir\src"

Write-Host "Server started. Press Ctrl+C to stop." -ForegroundColor Green
Write-Host "Note: MongoDB must be running on mongodb://127.0.0.1:27017" -ForegroundColor Yellow
"@
    Set-Content -Path (Join-Path $ServerDist "start.ps1") -Value $StartPs1 -Encoding UTF8
    Write-Host "  -> start.ps1" -ForegroundColor Gray

    Write-Host "[PACK] Server packed." -ForegroundColor Green
}

# ── 6. 打包 client ──
if ($Target -in @("all", "client")) {
    Write-Host "[PACK] Packing client..." -ForegroundColor Yellow
    $ClientDist = Join-Path $Dist "client"
    New-Item -ItemType Directory -Path $ClientDist -Force | Out-Null

    # engine/ - 复制 Python 引擎框架
    $ClientEngineSrc = Join-Path $Root "client\engine"
    $ClientEngineDist = Join-Path $ClientDist "engine"
    if (Test-Path $ClientEngineSrc) {
        Copy-Item $ClientEngineSrc $ClientEngineDist -Recurse -Force
        Write-Host "  -> engine/" -ForegroundColor Gray

        # 替换 pyclient 原生模块
        $PyclientDir = Join-Path $ClientEngineDist "pyclient"
        if (Test-Path $PyclientDir) {
            # 删除旧的不兼容 .pyd 文件
            Get-ChildItem $PyclientDir -Filter "*.pyd" | Remove-Item -Force
            $SrcPyclient = Join-Path $Root "client\target\$TargetDir\pyclient.dll"
            if (Test-Path $SrcPyclient) {
                Copy-Item $SrcPyclient (Join-Path $PyclientDir "pyclient.pyd") -Force
                Write-Host "  -> engine/pyclient/pyclient.pyd" -ForegroundColor Gray
            } else {
                Write-Host "[WARN] pyclient.dll not found at $SrcPyclient" -ForegroundColor Red
            }
        }
    }

    # 生成 client/run.py
    $ClientRunPy = @"
import sys, os, uuid

_dir = os.path.dirname(os.path.abspath(__file__))

from engine.app import app, client_event_handle
from engine.player import player
from engine.subentity import subentity
from engine.callback import callback

class ClientEventHandle(client_event_handle):
    def on_kick_off(self, prompt_info):
        print(prompt_info)
    def on_transfer_complete(self):
        print("on_transfer_complete")

playerImpl = None

class SamplePlayer(player):
    def __init__(self, entity_id):
        super().__init__("SamplePlayer", entity_id)
    def Creator(entity_id, description):
        global playerImpl
        playerImpl = SamplePlayer(entity_id)
        app().login(str(uuid.uuid4()), {})
        return playerImpl
    def update_player(self, argvs):
        print(f"SamplePlayer:{self.entity_id} update_player!")

def conn_callback(conn_id):
    app().login(str(uuid.uuid4()), {})

def main():
    _app = app()
    _app.build(ClientEventHandle())
    _app.register("SamplePlayer", SamplePlayer.Creator)
    _app.connect_ws("ws://127.0.0.1:8100", conn_callback)
    _app.run()

if __name__ == '__main__':
    main()
"@
    Set-Content -Path (Join-Path $ClientDist "run.py") -Value $ClientRunPy -Encoding UTF8
    Write-Host "  -> run.py" -ForegroundColor Gray

    Write-Host "[PACK] Client packed." -ForegroundColor Green
}

# ── 7. 生成 build_info.txt ──
$BuildTime = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
$BuildInfo = @"
Geese Engine Build Info
=======================
Build Time:    $BuildTime
Profile:       $Profile
Target:        $Target
Python:        $PyVersion ($PyTag)
Rust:          $(& rustc --version)
OS:            $([System.Environment]::OSVersion.VersionString)

Output: dist/
  desktop/   - pydesktop.pyd + run.py + run_launcher.py
  server/    - dbproxy.exe + gate.exe + pyhub.pyd + engine/ + config/ + start.ps1
  client/    - pyclient.pyd + engine/ + run.py

Usage:
  Desktop:  python dist/desktop/run.py [project_path]
  Server:   powershell -ExecutionPolicy Bypass -File dist/server/start.ps1
  Client:   python dist/client/run.py
"@
    Set-Content -Path (Join-Path $Dist "build_info.txt") -Value $BuildInfo -Encoding UTF8
    Write-Host "[INFO] build_info.txt generated." -ForegroundColor Green

# ── 8. 汇总 ──
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Build Complete!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Output: $Dist"
Write-Host ""
Write-Host "Usage:"
Write-Host "  Desktop:  python dist/desktop/run.py"
Write-Host "  Server:   powershell -ExecutionPolicy Bypass -File dist/server/start.ps1"
Write-Host "  Client:   python dist/client/run.py"
Write-Host ""

# 列出产物大小
Write-Host "Artifacts:" -ForegroundColor Gray
Get-ChildItem $Dist -Recurse -File | ForEach-Object {
    $rel = $_.FullName.Substring($Dist.Length + 1)
    $size = if ($_.Length -gt 1MB) { "{0:N1} MB" -f ($_.Length / 1MB) } else { "{0:N0} KB" -f ($_.Length / 1KB) }
    Write-Host "  $rel ($size)" -ForegroundColor DarkGray
}
