$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../src/update.ps1') -Mode Library
function Assert($Value,[string]$Message) { if (-not $Value) { throw $Message } }
function New-Fixture {
    return [pscustomobject]@{
        draft=$false; prerelease=$false; tag_name='v1.10.0'
        assets=@([pscustomobject]@{ name='MicKey.exe'; browser_download_url='https://github.com/Samsxx001/MicKey/releases/download/v1.10.0/MicKey.exe'; digest=('sha256:' + ('a'*64)); size=1000 })
    }
}
function Reject($Fixture,[string]$Message) {
    $rejected=$false
    try { $null=Get-VerifiedRelease $Fixture 'Samsxx001/MicKey' '1.3.0' } catch { $rejected=$true }
    Assert $rejected $Message
}
$r=Get-VerifiedRelease (New-Fixture) 'Samsxx001/MicKey' '1.3.0'
Assert ($r.Version -eq '1.10.0') 'Versions must compare numerically, not as text.'
Assert ($null -eq (Get-VerifiedRelease (New-Fixture) 'Samsxx001/MicKey' '1.10.0')) 'An equal version must not reinstall.'
Assert ($null -eq (Get-VerifiedRelease (New-Fixture) 'Samsxx001/MicKey' '2.0.0')) 'An older release must not downgrade.'
$r=New-Fixture; $r.prerelease=$true; Reject $r 'Prereleases must be rejected.'
$r=New-Fixture; $r.draft=$true; Reject $r 'Drafts must be rejected.'
$r=New-Fixture; $r.assets[0].browser_download_url='https://example.com/MicKey.exe'; Reject $r 'Third-party download origins must be rejected.'
$r=New-Fixture; $r.assets[0].browser_download_url='http://github.com/Samsxx001/MicKey/releases/download/v1.10.0/MicKey.exe'; Reject $r 'HTTP must be rejected.'
$r=New-Fixture; $r.assets[0].browser_download_url='https://github.com/other/project/releases/download/v1.10.0/MicKey.exe'; Reject $r 'A different repository must be rejected.'
$r=New-Fixture; $r.assets[0].digest=$null; Reject $r 'Missing digests must be rejected.'
$r=New-Fixture; $r.assets[0].digest='sha256:abc'; Reject $r 'Malformed digests must be rejected.'
$r=New-Fixture; $r.assets[0].size=0; Reject $r 'Empty assets must be rejected.'
$r=New-Fixture; $r.assets[0].size=52428801; Reject $r 'Oversized assets must be rejected.'
$r=New-Fixture; $r.assets=@($r.assets[0],$r.assets[0]); Reject $r 'Duplicate executable assets must be rejected.'
$r=New-Fixture; $r.tag_name='v1.10.0-beta'; Reject $r 'Non-stable tags must be rejected.'
$temp=Join-Path ([IO.Path]::GetTempPath()) ('mickey-hash-test-' + [guid]::NewGuid().ToString('N'))
try {
    [IO.File]::WriteAllBytes($temp,[byte[]](77,90,1,2,3,4))
    $hash=(Get-FileHash -LiteralPath $temp -Algorithm SHA256).Hash
    Test-DownloadedFile $temp $hash
    $rejected=$false
    try { Test-DownloadedFile $temp ('b'*64) } catch { $rejected=$true }
    Assert $rejected 'Corrupt downloads must be rejected.'
    [IO.File]::WriteAllBytes($temp,[byte[]](1,2,3,4))
    $hash=(Get-FileHash -LiteralPath $temp -Algorithm SHA256).Hash
    $rejected=$false
    try { Test-DownloadedFile $temp $hash } catch { $rejected=$true }
    Assert $rejected 'Non-executable payloads must be rejected.'
} finally { if (Test-Path -LiteralPath $temp) { [IO.File]::Delete($temp) } }
Write-Output 'Updater tests passed (17 checks).'
