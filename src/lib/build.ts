// PR용 시험판 설치 파일이면 "시험판 · PR 21"처럼 이름표가 붙는다(정식 배포판은 없음).
// PR 빌드(.github/workflows/pr-build.yml)가 VITE_BUILD_LABEL을 넣어 만든다.
export const BUILD_LABEL: string | null = (import.meta.env.VITE_BUILD_LABEL as string | undefined)?.trim() || null;
