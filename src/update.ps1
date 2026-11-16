param(
    [ValidateSet('Check','Install','Library')][string]$Mode = 'Check',
    [string]$Repository, [string]$CurrentVersion, [string]$Stage,
    [string]$Target, [int]$ParentId, [string]$ReadyEvent, [string]$ExpectedSha,
    [switch]$Quiet
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-VerifiedRelease($Release, [string]$Repo, [string]$Current) {
    if ($Repo -notmatch '^[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+$') { throw 'Invalid GitHub repository.' }
    if ($Release.draft -or $Release.prerelease) { throw 'Only stable published releases are supported.' }
    if ($Release.tag_name -notmatch '^v?(\d+\.\d+\.\d+)$') { throw 'Invalid release version.' }
    $version = $Matches[1]
    if ([version]$version -le [version]$Current) { return $null }
    $asset = @($Release.assets | Where-Object { $_.name -ceq 'MicKey.exe' })
    if ($asset.Count -ne 1) { throw 'Release does not contain exactly one MicKey.exe.' }
    $url = [uri]$asset[0].browser_download_url
    $expectedPath = "/$Repo/releases/download/$($Release.tag_name)/MicKey.exe"
    if ($url.Scheme -cne 'https' -or $url.Host -ine 'github.com' -or
        $url.AbsolutePath -cne $expectedPath -or $url.Query -or $url.Fragment -or $url.UserInfo -or $url.Port -ne 443) {
        throw 'Unexpected release download URL.'
    }
    if ($asset[0].digest -notmatch '^sha256:([a-fA-F0-9]{64})$') { throw 'GitHub did not provide a SHA-256 digest.' }
    $sha = $Matches[1].ToLowerInvariant()
    if ([long]$asset[0].size -le 0 -or [long]$asset[0].size -gt 52428800) { throw 'Unexpected release asset size.' }
    return [pscustomobject]@{ Version=$version; Url=$url.AbsoluteUri; Sha=$sha; Size=[long]$asset[0].size }
}

function Test-DownloadedFile([string]$Path, [string]$Sha) {
    if ($Sha -notmatch '^[a-fA-F0-9]{64}$') { throw 'Invalid SHA-256 digest.' }
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ine $Sha) { throw 'SHA-256 verification failed. The program was not replaced.' }
    $stream = [IO.File]::OpenRead($Path)
    try {
        if ($stream.ReadByte() -ne 77 -or $stream.ReadByte() -ne 90) { throw 'Download is not a Windows executable.' }
    } finally { $stream.Dispose() }
}

if ($Mode -eq 'Library') { return }
Add-Type -AssemblyName System.Windows.Forms
function Show-Info([string]$Text) { [void][Windows.Forms.MessageBox]::Show($Text,'MicKey',[Windows.Forms.MessageBoxButtons]::OK,[Windows.Forms.MessageBoxIcon]::Information) }
try {
    if ($Mode -eq 'Check') {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $headers = @{ Accept='application/vnd.github+json'; 'User-Agent'="MicKey/$CurrentVersion"; 'X-GitHub-Api-Version'='2022-11-28' }
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repository/releases/latest" -Headers $headers -TimeoutSec 30
        $next = Get-VerifiedRelease $release $Repository $CurrentVersion
        if ($null -eq $next) { Show-Info "MicKey $CurrentVersion เป็นรุ่นล่าสุดแล้ว"; exit 0 }
        $answer = [Windows.Forms.MessageBox]::Show("มี MicKey $($next.Version) พร้อมอัปเดต (รุ่นปัจจุบัน $CurrentVersion)`n`nต้องการดาวน์โหลดและติดตั้งหรือไม่? โปรแกรมจะปิดและเปิดใหม่ โดยเก็บปุ่มลัดเดิมไว้",'MicKey',[Windows.Forms.MessageBoxButtons]::YesNo,[Windows.Forms.MessageBoxIcon]::Question)
        if ($answer -ne [Windows.Forms.DialogResult]::Yes) { exit 0 }
        $file = Join-Path $Stage 'MicKey-update.exe'
        Invoke-WebRequest -UseBasicParsing -Uri $next.Url -OutFile $file -Headers @{ 'User-Agent'="MicKey/$CurrentVersion" } -TimeoutSec 120
        if ((Get-Item -LiteralPath $file).Length -ne $next.Size) { throw 'Downloaded file size does not match the release.' }
        Test-DownloadedFile $file $next.Sha
        [IO.File]::WriteAllText((Join-Path $Stage 'expected.sha256'),$next.Sha)
        exit 10
    }
    $file = Join-Path $Stage 'MicKey-update.exe'
    $Target = [IO.Path]::GetFullPath($Target)
    if ([IO.Path]::GetExtension($Target) -ine '.exe' -or -not (Test-Path -LiteralPath $Target -PathType Leaf)) { throw 'Invalid installed program path.' }
    Test-DownloadedFile $file $ExpectedSha
    $parent = Get-Process -Id $ParentId -ErrorAction Stop
    if ([IO.Path]::GetFullPath($parent.Path) -ine $Target) { throw 'The running process does not match the installed program.' }
    # Check directory write access before asking the main app to exit.
    $probe = Join-Path ([IO.Path]::GetDirectoryName($Target)) ('.mickey-update-' + [guid]::NewGuid().ToString('N'))
    [IO.File]::WriteAllText($probe,'test'); [IO.File]::Delete($probe)
    $ready = [Threading.EventWaitHandle]::OpenExisting($ReadyEvent)
    try { [void]$ready.Set() } finally { $ready.Dispose() }
    if (-not $parent.WaitForExit(30000)) { throw 'MicKey did not exit in time. Update cancelled.' }
    Test-DownloadedFile $file $ExpectedSha
    $backup = $Target + '.previous'
    $hadBackup = $false
    try {
        if (Test-Path -LiteralPath $backup) { [IO.File]::Delete($backup) }
        [IO.File]::Move($Target,$backup); $hadBackup = $true
        [IO.File]::Copy($file,$Target,$false)
        Test-DownloadedFile $Target $ExpectedSha
        Start-Process -FilePath $Target -WorkingDirectory ([IO.Path]::GetDirectoryName($Target))
    } catch {
        $failure = $_
        if ($hadBackup) {
            if (Test-Path -LiteralPath $Target) { [IO.File]::Delete($Target) }
            [IO.File]::Move($backup,$Target)
            Start-Process -FilePath $Target -WorkingDirectory ([IO.Path]::GetDirectoryName($Target))
        }
        throw $failure
    }
    # Keep .previous as a manual fallback; the next successful update replaces it.
    exit 0
} catch {
    if ($Quiet) { [Console]::Error.WriteLine($_.Exception.Message) }
    else { [void][Windows.Forms.MessageBox]::Show("อัปเดตไม่สำเร็จ: $($_.Exception.Message)`n`nดาวน์โหลดด้วยตัวเองได้ที่ https://github.com/$Repository/releases/latest",'MicKey',[Windows.Forms.MessageBoxButtons]::OK,[Windows.Forms.MessageBoxIcon]::Error) }
    exit 1
}
