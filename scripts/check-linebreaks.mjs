// 앱 화면 전체 줄바꿈 검사: 화면마다 글 요소의 실제 줄을 읽어 어색한 곳을 찾는다(CLAUDE.md 화면 규칙).
// 줄 머리의 "·", 괄호·따옴표 안에서 끊김, 낱말이 글자 사이에서 갈라짐, 마지막 줄에 낱말 하나만 남음.
//
// 준비: npm ci 후 가짜 데이터로 화면을 띄운다.  npx vite --port 5179 --strictPort
// 검사: node scripts/check-linebreaks.mjs        (기본 창 1100×720)
//       node scripts/check-linebreaks.mjs min    (가장 작은 창 760×520)
//       v = 여러 줄인 글도 모두 보이기, shot = 화면마다 사진(scripts/.linebreaks/)
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const { chromium } = await import("playwright").catch(() => import("/opt/node22/lib/node_modules/playwright/index.mjs"));
const S = path.join(path.dirname(fileURLToPath(import.meta.url)), ".linebreaks") + path.sep;
const B = "http://localhost:5179/";
const b = await chromium.launch(process.env.CHROMIUM ? { executablePath: process.env.CHROMIUM } : { executablePath: "/opt/pw-browsers/chromium" }).catch(() => chromium.launch());
const verbose = process.argv.includes("v");
const shots = process.argv.includes("shot");
if (shots) fs.mkdirSync(S, { recursive: true });

async function audit(p, name) {
  const res = await p.evaluate(() => {
    const SEL = "p,li,label,h1,h2,h3,h4,td,th,small,strong,b,em,span,div,a,button";
    const out = [];
    const isLeafish = (el) => {
      // 글이 바로 들어 있는 요소만(자식 블록이 없는)
      if (!el.offsetParent && getComputedStyle(el).position !== "fixed") return false;
      const hasText = [...el.childNodes].some((n) => n.nodeType === 3 && n.data.trim());
      if (!hasText) return false;
      for (const c of el.children) {
        const d = getComputedStyle(c).display;
        if (d === "block" || d === "flex" || d === "grid" || d === "list-item") return false;
      }
      return true;
    };
    const seen = new Set();
    for (const el of document.querySelectorAll(SEL)) {
      if (!isLeafish(el)) continue;
      // 안쪽 인라인 요소는 바깥 것에서 같이 읽는다
      let up = el.parentElement, skip = false;
      while (up) { if (seen.has(up)) { skip = true; break; } up = up.parentElement; }
      if (skip) continue;
      seen.add(el);
      const lines = [];
      const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
      let n, lastTop = null, line = "", prev = "";
      const midWord = [];
      const isWordChar = (c) => /[가-힣A-Za-z0-9]/.test(c);
      while ((n = walker.nextNode())) {
        for (let i = 0; i < n.data.length; i++) {
          const ch = n.data[i];
          const r = document.createRange(); r.setStart(n, i); r.setEnd(n, i + 1);
          const rect = r.getClientRects()[0];
          if (!rect || rect.width === 0) { line += ch; if (ch.trim()) prev = ch; else prev = " "; continue; }
          const mid = Math.round(rect.top + rect.height / 2);
          if (lastTop !== null && mid - lastTop > rect.height * 0.6) {
            // 빈칸 없이 글자와 글자 사이에서 줄이 바뀜 = 낱말이 갈라짐
            if (isWordChar(prev) && isWordChar(ch)) midWord.push(line.slice(-6) + "|" + n.data.slice(i, i + 6));
            lines.push(line); line = "";
          }
          lastTop = mid; line += ch; prev = ch;
        }
      }
      if (line) lines.push(line);
      const L = lines.map((l) => l.trim()).filter(Boolean);
      if (L.length < 2) continue;
      const flags = midWord.map((w) => "낱말이 갈라짐 " + w);
      for (let k = 0; k < L.length; k++) {
        const t = L[k];
        if (k > 0 && /^[·,.)\]’”:;!?…]/.test(t)) flags.push("줄 머리에 " + t[0]);
        if (k < L.length - 1 && /[(\[‘“·]$/.test(t)) flags.push("줄 끝에 " + t[t.length - 1]);
        const c = (x) => t.split(x).length - 1;
        if (c("(") !== c(")") || c("‘") !== c("’") || c("[") !== c("]") || c("“") !== c("”")) flags.push("괄호·따옴표 안에서 끊김");
      }
      const last = L[L.length - 1];
      if (!/\s/.test(last) && last.length <= 4) flags.push("마지막 줄에 한 낱말: " + last);
      out.push({ text: L.join(" ⏎ "), flags });
    }
    return out;
  });
  const bad = res.filter((r) => r.flags.length);
  console.log(`\n## ${name}: 여러 줄 ${res.length}개, 문제 ${bad.length}개`);
  for (const r of verbose ? res : bad) console.log("  " + (r.flags.length ? "!! " + [...new Set(r.flags)].join(", ") + " :: " : "   ") + r.text.slice(0, 220));
  if (shots) await p.screenshot({ path: S + "lb-" + name.replace(/[^\w가-힣-]/g, "_") + ".png" });
}

const sizes = process.argv.includes("min") ? [[760, 520]] : [[1100, 720]];
for (const [w, h] of sizes) {
  console.log(`\n================ ${w}x${h}`);
  // 처음 설정 6단계
  let p = await b.newPage({ viewport: { width: w, height: h } });
  p.on("pageerror", (e) => console.log("ERR", e.message));
  await p.goto(B); await p.waitForTimeout(900);
  for (let step = 1; step <= 6; step++) {
    await audit(p, `처음 설정 ${step}`);
    const next = p.getByRole("button", { name: /^다음$/ });
    if (await next.count()) { await next.first().click(); await p.waitForTimeout(500); }
  }
  await p.close();
  // 메인 창
  p = await b.newPage({ viewport: { width: w, height: h } });
  p.on("pageerror", (e) => console.log("ERR", e.message));
  await p.goto(B + "?mock=ready&classify=1&update=1"); await p.waitForTimeout(1200);
  await audit(p, "정리함");
  await p.locator(".ifold__tile").filter({ hasText: "수업" }).locator(".ifold__open").click(); await p.waitForTimeout(700);
  await audit(p, "정리함-폴더 파일");
  await p.locator(".ifold__bar button").first().click(); await p.waitForTimeout(400);
  await p.getByRole("button", { name: "자동 분류" }).click(); await p.waitForTimeout(900);
  await audit(p, "자동 분류(카드)");
  await p.locator(".dialog--classify").getByRole("button", { name: "목록으로 보기" }).click(); await p.waitForTimeout(500);
  await audit(p, "자동 분류(목록)");
  await p.locator(".dialog--classify").getByRole("button", { name: "카드로 보기" }).click();
  await p.locator(".dialog--classify").getByRole("button", { name: "취소" }).click(); await p.waitForTimeout(400);
  await p.getByRole("tab", { name: /최근 정리/ }).click(); await p.waitForTimeout(400);
  await audit(p, "정리함-최근 정리");
  for (const [menu, label] of [["정리 기록", "정리 기록"], ["파일 검색", "파일 검색"], ["이름 바꾸기", "이름 바꾸기"], ["정리 규칙", "정리 규칙"]]) {
    await p.locator(".sidebar__item", { hasText: menu }).click(); await p.waitForTimeout(800);
    if (menu === "파일 검색") { await p.locator("input").first().fill("수행"); await p.waitForTimeout(800); }
    await audit(p, label);
  }
  await p.locator(".sidebar__item", { hasText: "설정" }).click(); await p.waitForTimeout(800);
  for (const tab of ["정리 방식", "폴더", "파일 검색", "프로그램"]) {
    await p.locator(".settings__tab", { hasText: tab }).click(); await p.waitForTimeout(500);
    await audit(p, "설정-" + tab);
  }
  await p.close();
  // 새 파일 창
  p = await b.newPage({ viewport: { width: 460, height: 640 } });
  await p.goto(B + "?window=popup"); await p.waitForTimeout(900);
  await audit(p, "새 파일 창");
  await p.getByRole("button", { name: /^넣기$/ }).click(); await p.waitForTimeout(700);
  await audit(p, "새 파일 창-넣은 뒤");
  await p.close();
  // 내 PC 어디든: 카카오톡 받은 파일 + ⋯ 메뉴, 한꺼번에 생긴 파일 알림
  p = await b.newPage({ viewport: { width: 460, height: 640 } });
  await p.goto(B + "?window=popup&src=kakao"); await p.waitForTimeout(900);
  await p.getByRole("button", { name: "더 보기" }).click(); await p.waitForTimeout(300);
  await audit(p, "새 파일 창-⋯ 메뉴");
  await p.close();
  p = await b.newPage({ viewport: { width: 460, height: 640 } });
  await p.goto(B + "?window=popup&bulk=1"); await p.waitForTimeout(900);
  await audit(p, "새 파일 창-한꺼번에");
  await p.close();
}
await b.close();
