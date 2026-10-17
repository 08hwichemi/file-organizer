// 설명서 그림(docs/manual/img)을 지금 화면으로 다시 찍는다.
//
// 준비: npm ci 후 가짜 데이터로 화면을 띄운다(시험판 이름표 없이).
//   npx vite --port 5179 --strictPort
// 찍기: node docs/manual/tools/shots.mjs            (전부)
//       node docs/manual/tools/shots.mjs popup inbox  (일부: setup popup inbox classify more windows)
// 누를 곳은 주황 테두리(#E8590C)로 표시한다. Windows·브라우저 화면(r-*.webp)은 windows-screens.html을 찍는다.
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const { chromium } = await import("playwright").catch(() => import("/opt/node22/lib/node_modules/playwright/index.mjs"));
const HERE = path.dirname(fileURLToPath(import.meta.url));
const IMG = path.join(HERE, "..", "img") + path.sep;
const APP = "http://localhost:5179/";
const only = process.argv.slice(2);
const want = (k) => only.length === 0 || only.includes(k);

const b = await chromium.launch(process.env.CHROMIUM ? { executablePath: process.env.CHROMIUM } : { executablePath: "/opt/pw-browsers/chromium" }).catch(() => chromium.launch());
// inset: 창 끝까지 닿는 칸은 테두리가 잘리지 않게 안쪽에 그린다
const mark = (loc, inset = false) =>
  loc.first().evaluate((el, inset) => {
    el.style.outline = "4px solid #E8590C";
    el.style.outlineOffset = inset ? "-4px" : "3px";
  }, inset);
const unmark = (p) =>
  p.evaluate(() =>
    document.querySelectorAll("[style*='outline']").forEach((el) => {
      el.style.outline = "";
      el.style.outlineOffset = "";
    }),
  );
const page = async (w, h, url) => {
  const p = await b.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: 2 });
  p.on("pageerror", (e) => console.log("화면 오류:", e.message));
  await p.goto(APP + url);
  await p.evaluate(() => localStorage.clear());
  await p.reload();
  await p.waitForTimeout(900);
  return p;
};
// 그림은 WebP로 저장한다(PNG보다 3분의 1 크기라 설명서가 빨리 뜬다). 변환은 같은 브라우저의 canvas로.
let conv = null;
const save = async (png, name) => {
  conv ??= await b.newPage();
  const webp = await conv.evaluate(async (src) => {
    const img = new Image();
    img.src = "data:image/png;base64," + src;
    await img.decode();
    const c = document.createElement("canvas");
    c.width = img.naturalWidth;
    c.height = img.naturalHeight;
    c.getContext("2d").drawImage(img, 0, 0);
    return c.toDataURL("image/webp", 0.85).split(",")[1];
  }, png.toString("base64"));
  fs.writeFileSync(IMG + name, Buffer.from(webp, "base64"));
  console.log("  ", name);
};
const shot = async (p, name, target) => save(await (target ?? p).screenshot(), name);

// ---------- 처음 설정 6단계 ----------
if (want("setup")) {
  console.log("처음 설정");
  const p = await page(1000, 820, "");
  const card = p.locator(".setup__card");
  const next = async () => {
    await p.getByRole("button", { name: "다음", exact: true }).click();
    await p.waitForTimeout(350);
  };
  const choice = (text) => p.locator("label").filter({ hasText: text });
  await mark(choice("학교"));
  await shot(p, "fr1.webp", card);
  await unmark(p);
  await next();
  await mark(choice("문서 폴더에 새로 만들기"));
  await shot(p, "fr2.webp", card);
  await unmark(p);
  await next();
  await mark(choice("폴더를 정해서 시작"));
  await shot(p, "fr3.webp", card);
  await unmark(p);
  await next();
  await shot(p, "fr4.webp", card);
  await next();
  await mark(choice("바로 물어보기"));
  await shot(p, "fr5.webp", card);
  await unmark(p);
  await next();
  await mark(p.getByRole("button", { name: "시작하기" }));
  await shot(p, "fr6.webp", card);
  await p.close();
}

// ---------- 새 파일 창 ----------
if (want("popup")) {
  console.log("새 파일 창");
  const p = await page(460, 640, "?window=popup");
  await shot(p, "popup.webp");
  await shot(p, "pp1.webp");
  await mark(p.locator(".popup__suggest-btn"));
  await mark(p.getByRole("button", { name: "넣기", exact: true }));
  await shot(p, "pp2.webp");
  await unmark(p);
  await mark(p.locator(".popup__recent"), true);
  await shot(p, "pp3.webp");
  await unmark(p);
  await mark(p.getByRole("button", { name: "2학기" }).locator("xpath=.."));
  await shot(p, "pp4.webp");
  await unmark(p);
  await mark(p.getByRole("button", { name: "나중에 정리", exact: true }));
  await shot(p, "pp5.webp");
  await unmark(p);
  // 넣은 뒤: 최근 넣은 곳 '회의록'에 넣으면 규칙 카드가 나온다
  await p.locator(".popup__recent-btn").first().click();
  await p.waitForTimeout(250);
  await p.getByRole("button", { name: "넣기", exact: true }).click();
  await p.waitForTimeout(900);
  await mark(p.getByRole("button", { name: "되돌리기" }));
  await shot(p, "pp6.webp");
  await unmark(p);
  await p.getByRole("button", { name: "수행평가", exact: true }).click();
  await p.waitForTimeout(300);
  await mark(p.getByRole("button", { name: "수행평가", exact: true }));
  await mark(p.getByRole("button", { name: "규칙으로 저장" }));
  await shot(p, "pp7.webp");
  await p.close();
}

// ---------- 정리함 · 자동 분류 ----------
if (want("inbox") || want("classify")) {
  console.log("정리함 · 자동 분류");
  const p = await page(1100, 720, "?mock=ready&classify=1");
  // [지금 있는 파일 가져오기]는 목록 맨 아래에 있다
  const list = p.locator(".ifiles__list");
  await list.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await mark(p.getByRole("button", { name: "지금 있는 파일 가져오기" }));
  await shot(p, "ib2.webp");
  await unmark(p);
  await list.evaluate((el) => (el.scrollTop = 0));
  await mark(p.getByRole("button", { name: "자동 분류" }));
  await shot(p, "ib3.webp");
  await unmark(p);
  await p.getByRole("button", { name: "자동 분류" }).click();
  await p.waitForTimeout(1200);
  await shot(p, "classify.webp", p.locator(".dialog--classify"));
  await shot(p, "cl1.webp", p.locator(".dialog--classify"));
  const dlg = p.locator(".dialog--classify");
  const src = dlg.locator(".classify__name", { hasText: "수행평가_안내문.hwp" }).first();
  const dst = dlg.locator(".classify__card").filter({ hasText: "지필고사" }).first();
  await dst.scrollIntoViewIfNeeded();
  const sb = await src.boundingBox();
  const db = await dst.boundingBox();
  await p.mouse.move(sb.x + 30, sb.y + sb.height / 2);
  await p.mouse.down();
  await p.mouse.move(sb.x + 45, sb.y + sb.height / 2 + 6, { steps: 6 });
  await p.mouse.move(db.x + db.width / 2, db.y + 60, { steps: 25 });
  await p.waitForTimeout(300);
  await shot(p, "cl2.webp", p.locator(".dialog--classify"));
  await p.mouse.up();
  await p.waitForTimeout(500);
  console.log("   옮겨졌나:", (await dst.innerText()).includes("수행평가_안내문"));
  // 여러 개 골라 한꺼번에 끌기: 다른 창에서(이 창의 다음 그림이 바뀌지 않게)
  {
    const m = await page(1100, 720, "?mock=ready&classify=1");
    await m.getByRole("button", { name: "자동 분류" }).click();
    await m.waitForTimeout(1200);
    const md = m.locator(".dialog--classify");
    const board = md.locator(".classify__board .classify__card");
    const first = board.nth(0);
    const rows = first.locator(".classify__file");
    await rows.nth(0).click();
    await rows.nth(1).click();
    await m.waitForTimeout(200);
    await mark(md.locator(".classify__selbar"));
    const g = await rows.nth(1).boundingBox();
    const to = await board.nth(1).boundingBox();
    await m.mouse.move(g.x + 60, g.y + g.height / 2);
    await m.mouse.down();
    await m.mouse.move(g.x + 75, g.y + g.height / 2 + 6, { steps: 6 });
    // 끌리는 표시는 마우스 오른쪽 아래에 붙으므로 카드 왼쪽을 가리켜 창 안에 다 보이게
    await m.mouse.move(to.x + 70, to.y + 110, { steps: 25 });
    await m.waitForTimeout(300);
    await shot(m, "cl5.webp", md);
    await m.close();
  }
  await mark(dlg.getByRole("button", { name: /개 나누기/ }));
  await shot(p, "cl3.webp", p.locator(".dialog--classify"));
  await unmark(p);
  await dlg.getByRole("button", { name: /개 나누기/ }).click();
  await p.waitForTimeout(1000);
  await mark(p.getByText(/자동으로 나눴어요/).locator("xpath=.."));
  await shot(p, "cl4.webp");
  await unmark(p);

  // 정리한 뒤: 최근 정리 탭, 폴더 안 파일
  await p.waitForTimeout(10500); // 알림이 사라지게
  // 최근 정리 줄이 잘리지 않게 넓은 창에서, 왼쪽 메뉴는 빼고 찍는다
  await p.setViewportSize({ width: 1440, height: 780 });
  await p.waitForTimeout(400);
  const main = p.locator("main").first();
  await p.getByRole("tab", { name: /최근 정리/ }).click();
  await p.waitForTimeout(600);
  await mark(p.getByRole("tab", { name: /최근 정리/ }), true);
  await mark(p.locator(".recent__btn", { hasText: "되돌리기" }));
  await shot(p, "rc1.webp", main);
  await unmark(p);
  await p.locator(".recent__row").filter({ hasText: "수행평가" }).first().click();
  await p.waitForTimeout(900);
  await mark(p.locator(".ffiles__head"), true);
  await shot(p, "fd1.webp", p.locator(".ifold").first());
  await p.close();

  // 기존 폴더 다시 정리하기: [다른 폴더 정리하기] → 폴더 고르기(가짜) → 나누기 창
  const q = await page(1100, 720, "?mock=ready");
  await q.getByRole("button", { name: "다른 폴더 정리하기" }).click();
  await q.waitForTimeout(1200);
  await mark(q.locator(".classify__card").filter({ hasText: "새 폴더" }).first(), true);
  await shot(q, "rs1.webp", q.locator(".dialog--classify"));
  await q.close();
}

// ---------- 그 밖에: 파일 검색 · 이름 바꾸기 ----------
if (want("more")) {
  console.log("파일 검색 · 이름 바꾸기");
  let p = await page(1100, 680, "?mock=ready");
  await p.getByRole("button", { name: "파일 검색" }).click();
  await p.waitForTimeout(300);
  await p.keyboard.type("수행평가");
  await p.waitForTimeout(900);
  await p.locator(".finder__th--modified").click();
  await p.waitForTimeout(700);
  await shot(p, "search.webp", p.locator(".finder"));
  await p.close();

  p = await page(1100, 1400, "?mock=ready");
  await p.getByRole("button", { name: "이름 바꾸기" }).click();
  await p.waitForTimeout(400);
  await p.getByRole("button", { name: "폴더 고르기" }).click();
  await p.waitForTimeout(500);
  await p.getByLabel("첫 파일의 새 이름").fill("2026년 고다윤");
  await p.waitForTimeout(900);
  const s2 = await p.locator("#rename-step2").boundingBox();
  const prev = await p.getByRole("heading", { name: /미리보기/ }).boundingBox();
  const x = s2.x - 26;
  const y = s2.y - 24;
  await save(await p.screenshot({ clip: { x, y, width: 1100 - x - 26, height: prev.y + 250 - y } }), "rename.webp");
  await p.close();
}

// ---------- Windows·브라우저 화면(예시 그림) ----------
if (want("windows")) {
  console.log("Windows·브라우저 화면");
  const p = await b.newPage({ viewport: { width: 900, height: 900 }, deviceScaleFactor: 2 });
  await p.goto("file://" + path.join(HERE, "windows-screens.html"));
  await p.waitForTimeout(800);
  for (const id of ["r-addr", "r-assets", "r-done", "r-edge1", "r-edge2", "r-ss1", "r-ss2", "r-nsis1", "r-nsis2", "r-nsis3", "r-nsis4"]) {
    await shot(p, id + ".webp", p.locator("#" + id));
  }
  await p.close();
}

await b.close();
