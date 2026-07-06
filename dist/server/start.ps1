$Dir = $PSScriptRoot
Write-Host "Starting Geese Server..." -ForegroundColor Cyan

# 1. Start Consul
Write-Host "[1/5] Starting Consul..." -ForegroundColor Yellow
Start-Process "$Dir\dependences\consul\consul.exe" -ArgumentList "agent","-dev"

# 2. Start Redis
Write-Host "[2/5] Starting Redis..." -ForegroundColor Yellow
Start-Process "$Dir\dependences\redis\redis-server.exe" -WorkingDirectory "$Dir\dependences\redis"

# 3. Wait for services
Write-Host "[3/5] Waiting for Consul and Redis..." -ForegroundColor Yellow
Start-Sleep -Seconds 3

# 4. Start dbproxy and gate
Write-Host "[4/5] Starting dbproxy and gate..." -ForegroundColor Yellow
Start-Process "$Dir\bin\dbproxy.exe" -ArgumentList "$Dir\config\dbproxy.cfg" -WorkingDirectory "$Dir\bin"
Start-Process "$Dir\bin\gate.exe" -ArgumentList "$Dir\config\gate.cfg" -WorkingDirectory "$Dir\bin"

# 5. Wait for gate, then start hub scripts
Write-Host "[5/5] Starting hub (Python)..." -ForegroundColor Yellow
Start-Sleep -Seconds 3
Start-Process python -ArgumentList "$Dir\src\app.py","$Dir\config\player.cfg" -WorkingDirectory "$Dir\src"
Start-Sleep -Seconds 2
Start-Process python -ArgumentList "$Dir\src\rank_app.py","$Dir\config\rank.cfg" -WorkingDirectory "$Dir\src"

Write-Host "Server started. Press Ctrl+C to stop." -ForegroundColor Green
Write-Host "Note: MongoDB must be running on mongodb://127.0.0.1:27017" -ForegroundColor Yellow
