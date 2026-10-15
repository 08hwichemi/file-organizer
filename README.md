# 파일 정리 도우미

바탕화면·다운로드에 새 파일이 저장되면 작은 창을 띄워, 클릭 한두 번으로 원하는 폴더에 넣어 주는 Windows 프로그램입니다.
쓰면서 "다음에도 이런 파일은 여기에"라는 규칙이 쌓여, 점점 손이 덜 가게 됩니다.

- 기획서: [docs/SPEC.md](docs/SPEC.md)
- 화면 시안: [docs/design](docs/design)

> 개발 중입니다. 설치 파일: https://github.com/08hwichemi/file-organizer-releases/releases/latest (`-setup.exe` 파일). 한 번 설치하면 이후에는 프로그램이 스스로 업데이트합니다.

## 개발

필요한 것: Node.js 22, Rust(stable). 리눅스에서는 `libwebkit2gtk-4.1-dev librsvg2-dev`도 설치한다.

```bash
npm install
npm run dev            # 브라우저에서 화면만 보기 (가짜 데이터: src/mocks/, 주소 뒤 ?mock=ready 면 처음 설정 건너뜀)
npm run tauri dev      # 실제 프로그램 창으로 실행
cd src-tauri && cargo test   # Rust 단위 테스트
```

- 폴더: `src/`(React 화면), `src-tauri/`(Rust), `docs/`(기획서·시안), `.github/workflows/`(빌드).
- PR을 올리면 GitHub Actions가 Windows 설치 파일을 만든다. PR의 Checks → "PR 빌드" → 아래 Artifacts에서 받는다.
- main에 병합하면 `release.yml`이 새 버전을 공개 저장소 Releases에 올리고, 설치된 프로그램이 스스로 업데이트한다. 필요한 Secrets는 `release.yml` 맨 위 설명 참고.
