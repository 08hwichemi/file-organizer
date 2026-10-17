# CLAUDE.md

이 저장소는 **AutoFolder**(오토폴더) — 바탕화면·다운로드에 새 파일이 저장되면 팝업을 띄워 원하는 폴더로 정리해 주는 Windows 프로그램이다.

- 무엇을 만드는지: `docs/SPEC.md` (반드시 먼저 읽는다)
- 화면 시안: `docs/design/popup.dc.html`(새 파일 팝업), `docs/design/inbox.dc.html`(정리함). 색·간격·구성은 시안을 따른다.
- 사용자는 고등학교 교사이고, 이 프로그램을 다른 선생님들께 나눠 줄 예정이다. 설명과 커밋 메시지·PR 설명은 **한국어**로 쓴다.

## 기술 스택

- Tauri 2 + React 18 + TypeScript + Vite
- Rust: `notify`(감시), `rusqlite`(SQLite), Tauri 플러그인(autostart, single-instance, updater, global-shortcut, dialog)
- 화면: Zustand, dnd-kit, Lucide 아이콘, Pretendard 폰트(번들에 포함, 외부 CDN 금지)

## 작업 방식

- `docs/SPEC.md` 9장의 **개발 단계 순서**대로, 단계마다 브랜치와 PR 하나. PR 설명에 그 단계의 "완료 기준"과 PC에서 확인할 방법을 적는다.
- 이 작업은 주로 클라우드(리눅스)에서 한다. Windows 창·트레이·Everything은 여기서 실행할 수 없다.
  - 클라우드에서 할 것: React 화면은 브라우저 + 가짜 데이터(`src/mocks/`)로 확인, Rust 로직은 단위 테스트(`cargo test`), 컴파일 확인.
  - Windows 확인: GitHub Actions(windows-latest)가 PR마다 설치 파일을 아티팩트로 만든다. 사용자가 받아서 설치해 확인한다. PR 설명에 무엇을 눌러 보면 되는지 단계별로 적는다.
- Windows 전용 코드는 `#[cfg(windows)]`로 감싸서 리눅스에서도 컴파일되게 한다.
- **화면을 바꾸기 전에 시안 이미지부터 보내 확인받는다**(사용자 요청). 코드는 그대로 두고 가짜 데이터 화면에 CSS·DOM을 임시로 덧씌워 찍은 사진을 보여 주고, "좋아"가 나온 뒤에 만든다.
- 글 줄바꿈 검사: `npx vite --port 5179 --strictPort`로 가짜 데이터 화면을 띄우고 `node scripts/check-linebreaks.mjs`(가장 작은 창은 `min`). 화면 글을 바꾸면 돌려서 "문제 0개"를 확인한다.
- 사용 설명서·설치 안내 이미지(SPEC 7장): 원본은 `docs/manual`, `docs/install-image`. 화면이 바뀌면 `node docs/manual/tools/shots.mjs`로 그림을 다시 찍고, 설명서는 같은 주소(https://claude.ai/artifact/UjLpBr6AA33DNrLko6JSzG)에 다시 올린다(글꼴은 `node_modules/pretendard`의 PretendardVariable.woff2를 같이 올림).
- 결정이 SPEC과 달라지면 `docs/SPEC.md`도 같은 PR에서 고친다.

## 반드시 지킬 것

- **사용자 파일·폴더를 영구 삭제하는 코드를 만들지 않는다.** 지우기는 Windows 휴지통으로 보내기만 한다(프로그램의 되돌리기·휴지통에서 복원 가능). 옮길 때는 덮어쓰지 않고 `이름 (2).ext`로 번호를 붙인다.
- 모든 이동·이름 바꾸기·휴지통 보내기는 기록하거나 되돌릴 수 있어야 한다.
- 저장이 끝나지 않은 파일(다운로드 중, 임시 파일)은 건드리지 않는다. 기준은 SPEC 5장.
- 네트워크는 업데이트 확인(GitHub Releases)에만 쓴다. 파일 이름·경로 등 사용자 정보를 밖으로 보내지 않는다.
- Everything은 공식 SDK로만 연결한다. 역분석하지 않는다. MIT 라이선스 고지를 포함한다.
- 업데이트 서명 개인 키를 저장소에 커밋하지 않는다(GitHub Secrets만).

## 화면 규칙

- 깔끔하고 모던하게, 처음 쓰는 사람도 설명 없이 쓸 수 있게. 유치한 장식·이모지·그라데이션 금지.
- 한글이 단어 중간에서 줄바꿈되면 안 된다: 전역 `word-break: keep-all; overflow-wrap: anywhere;`.
- 버튼 줄에서 버튼 하나만 다음 줄로 내려가면 안 된다: 버튼은 `white-space: nowrap`, 버튼 줄은 줄바꿈하지 않게(`flex-wrap: nowrap`) 하고 공간이 모자라면 글자를 줄이거나 배치를 바꾼다.
- 폴더는 목록이 아니라 **탐색기처럼 폴더 아이콘 격자**로 보여 준다(누르면 안으로, 경로 표시로 위로).
- 주요 색: 강조 `#2B59C3`, 글자 `#17191C`, 보조 글자 `#5F6670`, 선 `#E4E6EA`, 배경 `#F4F5F7`, 폴더 `#E3B23C`/`#F5CB5C`. CSS 변수로 모아 둔다.
- 버튼·입력칸 높이 최소 36px, 주요 버튼 44px.
