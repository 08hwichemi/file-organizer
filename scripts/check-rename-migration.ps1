# 이름 바꾸기 넘어오기 확인(PR 빌드, windows-latest에서 실행):
# 예전 이름 "파일 정리 도우미"(0.1.18)를 설치한 뒤, 새 설치 파일(AutoFolder)을 자동 업데이트처럼(/UPDATE) 깔아
# 예전 것이 깔끔히 넘어가고 사용자 자료·바로 가기는 그대로 이어지는지 본다.
param([Parameter(Mandatory = $true)][string]$NewInstaller)
$ErrorActionPreference = "Stop"

$oldName = "파일 정리 도우미"
$oldUrl = "https://github.com/08hwichemi/file-organizer-releases/releases/download/v0.1.18/file-organizer_0.1.18_x64-setup.exe"
$oldDir = Join-Path $env:LOCALAPPDATA $oldName
$newDir = Join-Path $env:LOCALAPPDATA "AutoFolder"
$data = Join-Path $env:APPDATA "com.hwichemi.file-organizer"
$uninst = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall"
$run = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$programs = [Environment]::GetFolderPath("Programs")
$desktop = [Environment]::GetFolderPath("Desktop")
$failed = @()
function Check([bool]$ok, [string]$what) {
  if ($ok) { Write-Host "  맞음  $what" } else { Write-Host "  틀림  $what"; $script:failed += $what }
}
function Target([string]$lnk) { (New-Object -ComObject WScript.Shell).CreateShortcut($lnk).TargetPath }

# 1) 예전 버전 설치 + 쓰던 상태 흉내(사용자 자료, 이름 붙은 자동 실행)
$old = Join-Path $env:RUNNER_TEMP "old-setup.exe"
Invoke-WebRequest $oldUrl -OutFile $old
Start-Process $old -ArgumentList "/S" -Wait
if (-not (Test-Path "$oldDir\file-organizer.exe")) { throw "예전 버전이 설치되지 않았어요: $oldDir" }
New-Item -ItemType Directory -Force $data | Out-Null
Set-Content "$data\marker.txt" "keep"
New-ItemProperty $run -Name $oldName -Value "`"$oldDir\file-organizer.exe`" --autostart" -Force | Out-Null
$hadStart = Test-Path "$programs\$oldName.lnk"
$hadDesktop = Test-Path "$desktop\$oldName.lnk"
Write-Host "예전 설치: 시작 메뉴 바로 가기 $hadStart, 바탕화면 바로 가기 $hadDesktop"

# 2) 새 설치 파일을 자동 업데이트처럼(/UPDATE: 바로 가기를 새로 만들지 않음)
Start-Process $NewInstaller -ArgumentList "/S", "/UPDATE" -Wait

# 3) 확인
Check (Test-Path "$newDir\file-organizer.exe") "새 폴더에 설치됨($newDir)"
Check (-not (Test-Path "$oldDir\file-organizer.exe")) "예전 실행 파일 지워짐"
Check (-not (Test-Path $oldDir)) "빈 예전 설치 폴더 지워짐"
Check (-not (Test-Path "$uninst\$oldName")) "프로그램 추가/제거의 예전 항목 지워짐"
Check (Test-Path "$uninst\AutoFolder") "프로그램 추가/제거에 AutoFolder"
Check ($null -eq (Get-ItemProperty $run -Name $oldName -ErrorAction SilentlyContinue)) "예전 이름의 자동 실행 지워짐"
Check (-not (Test-Path "$programs\$oldName.lnk")) "예전 시작 메뉴 바로 가기 없어짐"
Check (-not (Test-Path "$desktop\$oldName.lnk")) "예전 바탕화면 바로 가기 없어짐"
if ($hadStart) {
  Check ((Test-Path "$programs\AutoFolder.lnk") -and ((Target "$programs\AutoFolder.lnk") -eq "$newDir\file-organizer.exe")) "시작 메뉴 바로 가기가 AutoFolder(새 실행 파일)로"
}
if ($hadDesktop) {
  Check ((Test-Path "$desktop\AutoFolder.lnk") -and ((Target "$desktop\AutoFolder.lnk") -eq "$newDir\file-organizer.exe")) "바탕화면 바로 가기가 AutoFolder(새 실행 파일)로"
}
Check ((Test-Path "$data\marker.txt") -and ((Get-Content "$data\marker.txt") -eq "keep")) "사용자 자료 그대로"

if ($failed.Count -gt 0) { throw "넘어오기 확인 실패: $($failed -join ', ')" }
Write-Host "넘어오기 확인 끝: 모두 맞음"
