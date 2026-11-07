$ErrorActionPreference = 'Stop'
$user = [Environment]::UserName
$account = Get-LocalUser -Name $user
$passwordExpired = $null -ne $account.PasswordExpires -and $account.PasswordExpires -le (Get-Date)
$desktopReady = [bool]@(Get-Process explorer -IncludeUserName -ErrorAction SilentlyContinue | Where-Object UserName -Like "*\$user").Count
$applied = @(& net.exe accounts /maxpwage:unlimited 2>&1)
if ($LASTEXITCODE -ne 0) { throw "Could not set unlimited VM password age (net.exe exit $LASTEXITCODE): $($applied -join ' ')" }
$policy = @(& net.exe accounts 2>&1)
if ($LASTEXITCODE -ne 0) { throw "Could not verify VM password age (net.exe exit $LASTEXITCODE): $($policy -join ' ')" }
if (!($policy -match '^\s*Maximum password age \(days\):\s+Unlimited\s*$')) { throw "VM maximum password age is not Unlimited: $($policy -join ' ')" }
@{password_expired=$passwordExpired;desktop_ready=$desktopReady} | ConvertTo-Json -Compress
