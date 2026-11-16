$ErrorActionPreference='Stop'
$root=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../work'))
[void][IO.Directory]::CreateDirectory($root)
$case=Join-Path $root ('install-test-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($case)
$rustArgs=@()
if ($env:MICKEY_TEST_RUST_TOOLCHAIN) { $rustArgs+=('+'+$env:MICKEY_TEST_RUST_TOOLCHAIN) }
$old=Join-Path $case 'old.exe'; $new=Join-Path $case 'new.exe'
$parent=$null; $installer=$null
function Assert($Value,[string]$Message) { if (-not $Value) { throw $Message } }
try {
    & rustc @rustArgs --cfg fixture_old (Join-Path $PSScriptRoot 'fixtures/process.rs') -o $old
    if ($LASTEXITCODE -ne 0) { throw 'Old fixture build failed.' }
    & rustc @rustArgs (Join-Path $PSScriptRoot 'fixtures/process.rs') -o $new
    if ($LASTEXITCODE -ne 0) { throw 'New fixture build failed.' }
    foreach ($rollback in @($false,$true)) {
        $folder=Join-Path $case ([string]$rollback)
        [void][IO.Directory]::CreateDirectory($folder)
        $stage=Join-Path $folder 'stage'; [void][IO.Directory]::CreateDirectory($stage)
        $target=Join-Path $folder 'MicKey.exe'
        $payload=Join-Path $stage 'MicKey-update.exe'
        Copy-Item -LiteralPath $old -Destination $target
        if ($rollback) { [IO.File]::WriteAllBytes($payload,[byte[]](77,90,0,0,1,2)) }
        else { Copy-Item -LiteralPath $new -Destination $payload }
        $oldSha=(Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
        $newSha=(Get-FileHash -LiteralPath $payload -Algorithm SHA256).Hash
        [IO.File]::WriteAllText((Join-Path $folder 'shortcut.txt'),'Ctrl')
        $parent=Start-Process -FilePath $target -ArgumentList '--hold' -WindowStyle Hidden -PassThru
        $null=$parent.Handle
        $eventName='Local\MicKey.UpdateTest.'+[guid]::NewGuid().ToString('N')
        $ready=[Threading.EventWaitHandle]::new($false,[Threading.EventResetMode]::ManualReset,$eventName)
        try {
            $script=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../src/update.ps1'))
            $arguments=@('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',('"'+$script+'"'),'-Mode','Install','-Repository','Samsxx001/MicKey','-Stage',('"'+$stage+'"'),'-Target',('"'+$target+'"'),'-ParentId',$parent.Id,'-ReadyEvent',$eventName,'-ExpectedSha',$newSha,'-Quiet')
            $installer=Start-Process -FilePath powershell.exe -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $folder 'error.txt')
            $null=$installer.Handle
            Assert ($ready.WaitOne(15000)) 'Installer did not signal readiness.'
            Assert (-not $parent.HasExited) 'Installer must not terminate the running app itself.'
            $parent.Kill(); $parent.WaitForExit()
            Assert ($installer.WaitForExit(15000)) 'Installer timed out.'
            $installer.Refresh()
            $marker=[IO.Path]::ChangeExtension($target,'started')
            $deadline=[DateTime]::UtcNow.AddSeconds(5)
            while (-not (Test-Path -LiteralPath $marker) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 50 }
            if ($rollback) {
                Assert ($installer.ExitCode -eq 1) 'A restart failure must report an error.'
                Assert ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -eq $oldSha) 'Failed updates must restore the previous executable.'
                Assert ((Get-Content -LiteralPath $marker -Raw) -eq 'old') 'Failed updates must restart the old version.'
            } else {
                Assert ($installer.ExitCode -eq 0) ('Installation must succeed: ' + (Get-Content -LiteralPath (Join-Path $folder 'error.txt') -Raw))
                Assert ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -eq $newSha) 'Installed binary hash differs.'
                Assert ((Get-FileHash -LiteralPath ($target+'.previous') -Algorithm SHA256).Hash -eq $oldSha) 'Previous version backup differs.'
                Assert ((Get-Content -LiteralPath $marker -Raw) -eq 'new') 'New version did not restart.'
            }
            Assert ((Get-Content -LiteralPath (Join-Path $folder 'shortcut.txt') -Raw) -eq 'Ctrl') 'Updates must preserve settings.'
        } finally { $ready.Dispose() }
    }
    Write-Output 'Updater install and rollback integration tests passed.'
} finally {
    foreach ($process in @($parent,$installer)) {
        if ($null -ne $process -and -not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
    }
    $resolved=[IO.Path]::GetFullPath($case)
    if (-not $resolved.StartsWith($root+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe test cleanup path.' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
