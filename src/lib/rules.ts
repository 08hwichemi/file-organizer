// 정리 규칙을 화면에 보여 줄 글자로 바꾸는 도우미(SPEC 4장).
import { friendlyName, type KnownFolders } from "./paths";
import type { DestSegment, Rule } from "./types";

const quote = (w: string) => `‘${w}’`;

/** 조건: "파일 이름에 ‘수행평가’ 포함 · .hwp 파일 · 다운로드에서 온 파일" */
export function ruleCondition(
  rule: Pick<Rule, "words" | "extensions" | "sourceDir">,
  known: KnownFolders | null,
): string {
  const parts: string[] = [];
  if (rule.words.length > 0) {
    parts.push(
      `파일 이름에 ${rule.words.map(quote).join(", ")}${rule.words.length > 1 ? " 모두" : ""} 포함`,
    );
  }
  if (rule.extensions.length > 0) parts.push(`${rule.extensions.map((e) => `.${e}`).join("·")} 파일`);
  if (rule.sourceDir) parts.push(`${friendlyName(rule.sourceDir, known)}에서 온 파일`);
  return parts.join(" · ");
}

/** 짧은 이름(팝업 추천 칸): "이름의 ‘수행평가’ 규칙", "‘.hwp’ 파일 규칙" */
export function ruleName(rule: Pick<Rule, "words" | "extensions">): string {
  if (rule.words.length > 0) return `이름의 ${rule.words.map(quote).join("·")} 규칙`;
  return `${rule.extensions.map((e) => quote(`.${e}`)).join("·")} 파일 규칙`;
}

/** 저장 위치: "정리 폴더 › 평가 › 수행평가 › [연도] › [학기]" */
export function segmentsLabel(rootName: string, segments: DestSegment[], split: boolean): string {
  return [rootName, ...segments.map((s) => (split && s.template ? s.template : s.name))].join(" › ");
}

/** 나눌 수 있는 폴더 종류(체크 상자 글자용): "연도·학기", "연도·학기·학급" 중 들어 있는 것 */
export function splitKinds(segments: DestSegment[]): string {
  const all = segments.map((s) => s.template ?? "").join("");
  return [
    all.includes("[연도]") ? "연도" : "",
    all.includes("[학기]") ? "학기" : "",
    all.includes("[학급]") ? "학급" : "",
  ]
    .filter(Boolean)
    .join("·");
}

/** 쉼표·띄어쓰기로 적은 확장자를 나눈다: ".hwp, hwpx" → ["hwp", "hwpx"] */
export function parseExtensions(text: string): string[] {
  return text
    .split(/[\s,]+/)
    .map((e) => e.replace(/^\.+/, "").toLowerCase())
    .filter(Boolean);
}
