# Everything SDK(voidtools 공식 배포, MIT 라이선스)에서 Everything64.dll을 받아 설치 파일에 넣는다(SPEC 6장).
# GitHub Actions(windows-latest)에서 설치 파일을 만들기 전에 부른다.
# 받지 못해도 설치 파일은 만든다: 그때 프로그램은 Everything 없이 자체 색인(정리 폴더·지켜볼 폴더)으로 찾는다.
$ErrorActionPreference = "Stop"

$dest = Join-Path $PSScriptRoot "..\src-tauri\resources"
$zip = Join-Path $env:RUNNER_TEMP "everything-sdk.zip"
$dir = Join-Path $env:RUNNER_TEMP "everything-sdk"

try {
    Invoke-WebRequest -Uri "https://www.voidtools.com/Everything-SDK.zip" -OutFile $zip -UseBasicParsing
    Expand-Archive -Path $zip -DestinationPath $dir -Force
    $dll = Get-ChildItem -Path $dir -Recurse -Filter "Everything64.dll" | Select-Object -First 1
    if (-not $dll) { throw "SDK 안에 Everything64.dll이 없어요" }
    Copy-Item $dll.FullName (Join-Path $dest "Everything64.dll") -Force
    $hash = (Get-FileHash (Join-Path $dest "Everything64.dll") -Algorithm SHA256).Hash
    Write-Host "Everything64.dll 넣음 (SHA256 $hash)"
} catch {
    Write-Host "::warning::Everything SDK를 받지 못했어요. 이번 설치 파일은 자체 색인으로만 검색해요: $_"
}
