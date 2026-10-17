// 사용 설명서 웹페이지 묶음을 만든다(GitHub Pages에 올릴 것).
// 사용: node docs/manual/tools/site.mjs <내보낼 폴더>      (npm ci 후: 글꼴을 node_modules/pretendard에서 가져온다)
// 올리는 곳: 공개 저장소 08hwichemi/file-organizer-releases의 gh-pages 가지
//   → https://08hwichemi.github.io/file-organizer-releases/  (.github/workflows/manual.yml이 main 병합 때마다 올림)
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const MANUAL = path.join(HERE, "..");
const ROOT = path.join(MANUAL, "..", "..");
const out = process.argv[2];
if (!out) {
  console.error("내보낼 폴더를 적어 주세요: node docs/manual/tools/site.mjs <폴더>");
  process.exit(1);
}

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(path.join(out, "img"), { recursive: true });
fs.copyFileSync(path.join(MANUAL, "index.html"), path.join(out, "index.html"));
for (const f of fs.readdirSync(path.join(MANUAL, "img"))) {
  fs.copyFileSync(path.join(MANUAL, "img", f), path.join(out, "img", f));
}
// 메신저로 보내는 한 장짜리 설치 안내도 같은 곳에서 받을 수 있게
fs.copyFileSync(path.join(ROOT, "docs", "install-image", "install-guide.png"), path.join(out, "install-guide.png"));

// 글꼴: 글자 조각(dynamic subset) — 화면에 나온 글자가 든 조각만 받는다
const font = path.join(ROOT, "node_modules", "pretendard", "dist", "web", "variable");
fs.mkdirSync(path.join(out, "fonts"), { recursive: true });
fs.copyFileSync(path.join(font, "pretendardvariable-dynamic-subset.css"), path.join(out, "fonts", "pretendardvariable-dynamic-subset.css"));
fs.cpSync(path.join(font, "woff2-dynamic-subset"), path.join(out, "fonts", "woff2-dynamic-subset"), { recursive: true });

// GitHub Pages가 파일을 그대로 내보내게(Jekyll 처리 끄기)
fs.writeFileSync(path.join(out, ".nojekyll"), "");
console.log("설명서 묶음:", out);
