// install-image.html을 1080px 폭, 전체 페이지로 찍어 install-guide.png를 만든다(npm ci 후).
import { fileURLToPath } from "node:url";
import path from "node:path";

const { chromium } = await import("playwright").catch(() => import("/opt/node22/lib/node_modules/playwright/index.mjs"));
const HERE = path.dirname(fileURLToPath(import.meta.url));
const b = await chromium.launch(process.env.CHROMIUM ? { executablePath: process.env.CHROMIUM } : { executablePath: "/opt/pw-browsers/chromium" }).catch(() => chromium.launch());
const p = await b.newPage({ viewport: { width: 1080, height: 1600 } });
await p.goto("file://" + path.join(HERE, "install-image.html"));
await p.waitForTimeout(800);
await p.screenshot({ path: path.join(HERE, "install-guide.png"), fullPage: true });
await b.close();
