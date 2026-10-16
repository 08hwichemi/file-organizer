#!/usr/bin/env bash
# 배포용 파일 준비: 설치 파일·서명을 영문 이름으로 복사하고 자동 업데이트용 latest.json을 만든다.
# 사용: prepare-release.sh <nsis 폴더> <내보낼 폴더> <버전> <다운로드 주소 앞부분>
set -euo pipefail

NSIS_DIR="$1"
OUT="$2"
VERSION="$3"
BASE_URL="$4"

shopt -s nullglob
found=("$NSIS_DIR"/*_"${VERSION}"_x64-setup.exe)
if [ "${#found[@]}" -ne 1 ]; then
  echo "설치 파일을 하나만 찾아야 하는데 ${#found[@]}개를 찾았어요: ${found[*]:-없음}" >&2
  exit 1
fi
SRC="${found[0]}"
if [ ! -f "$SRC.sig" ]; then
  echo "업데이트 서명 파일이 없어요: $SRC.sig" >&2
  exit 1
fi

NAME="AutoFolder_${VERSION}_x64-setup.exe"
mkdir -p "$OUT"
cp "$SRC" "$OUT/$NAME"
cp "$SRC.sig" "$OUT/$NAME.sig"

# latest.json: 설치된 프로그램이 이 파일을 보고 새 버전을 받는다(Tauri updater 형식).
VERSION="$VERSION" URL="$BASE_URL/$NAME" SIG_FILE="$OUT/$NAME.sig" node -e '
  const fs = require("fs");
  const signature = fs.readFileSync(process.env.SIG_FILE, "utf8").trim();
  const entry = { signature, url: process.env.URL };
  const manifest = {
    version: process.env.VERSION,
    notes: "새 버전",
    pub_date: new Date().toISOString(),
    platforms: { "windows-x86_64": entry, "windows-x86_64-nsis": entry },
  };
  fs.writeFileSync(process.argv[1], JSON.stringify(manifest, null, 2) + "\n");
' "$OUT/latest.json"

echo "준비한 파일:"
ls -l "$OUT"
