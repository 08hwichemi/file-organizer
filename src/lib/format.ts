// 화면에 보여 줄 글자 만들기

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)}KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)}MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)}GB`;
}

export function formatAgo(ms: number, now = Date.now()): string {
  const minutes = Math.floor((now - ms) / 60_000);
  if (minutes < 1) return "방금";
  if (minutes < 60) return `${minutes}분 전`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}시간 전`;
  const days = Math.floor(hours / 24);
  if (days === 1) return "어제";
  return `${days}일 전`;
}

/** 이름과 확장자 나누기: "보고서.final.hwp" → ["보고서.final", ".hwp"] */
export function splitName(name: string): [string, string] {
  const i = name.lastIndexOf(".");
  return i > 0 ? [name.slice(0, i), name.slice(i)] : [name, ""];
}

interface Badge {
  label: string;
  bg: string;
  fg: string;
}

// 시안(docs/design)의 형식 배지 색
const BADGE_COLORS: Record<string, [string, string]> = {
  HWP: ["#E7EEFB", "#2B59C3"],
  DOC: ["#E7EEFB", "#2B59C3"],
  XLS: ["#E3F2E9", "#1B6E43"],
  PDF: ["#FBEAE7", "#A8352A"],
  IMG: ["#F0EAFB", "#6541B5"],
  PPT: ["#FCEEE2", "#A64C14"],
};

const FAMILY: Record<string, string> = {
  HWP: "HWP", HWPX: "HWP", DOC: "DOC", DOCX: "DOC", TXT: "DOC",
  XLS: "XLS", XLSX: "XLS", CSV: "XLS",
  PDF: "PDF",
  JPG: "IMG", JPEG: "IMG", PNG: "IMG", GIF: "IMG", HEIC: "IMG", WEBP: "IMG",
  PPT: "PPT", PPTX: "PPT",
};

export function fileBadge(name: string): Badge {
  const ext = splitName(name)[1].slice(1).toUpperCase();
  const [bg, fg] = BADGE_COLORS[FAMILY[ext] ?? ""] ?? ["#EEF0F2", "#3D434B"];
  return { label: ext.slice(0, 4) || "파일", bg, fg };
}

/** 조사 '로/으로': 받침이 없거나 ㄹ이면 '로'. "공문" → "공문으로", "1학기" → "1학기로" */
export function withRo(word: string): string {
  const code = word.charCodeAt(word.length - 1) - 0xac00;
  if (code < 0 || code > 11171) return `${word}로`;
  const final = code % 28;
  return final === 0 || final === 8 ? `${word}로` : `${word}으로`;
}

/** 날짜: 오늘이면 "오늘 14:05", 올해면 "10월 16일", 그 밖에는 "2025. 3. 2." */
export function formatDate(ms: number, now = new Date()): string {
  const d = new Date(ms);
  if (d.toDateString() === now.toDateString()) {
    return `오늘 ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  }
  if (d.getFullYear() === now.getFullYear()) return `${d.getMonth() + 1}월 ${d.getDate()}일`;
  return `${d.getFullYear()}. ${d.getMonth() + 1}. ${d.getDate()}.`;
}

/** 탐색기의 "수정한 날짜"처럼: "2026-10-16 오후 8:53" */
export function formatDateTime(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  const h = d.getHours();
  const half = h < 12 ? "오전" : "오후";
  const h12 = h % 12 === 0 ? 12 : h % 12;
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${half} ${h12}:${pad(d.getMinutes())}`;
}

const TYPE_NAMES: Record<string, string> = {
  HWP: "한글 문서", HWPX: "한글 문서",
  DOC: "Word 문서", DOCX: "Word 문서",
  XLS: "Excel 워크시트", XLSX: "Excel 워크시트", XLSM: "Excel 워크시트", CSV: "CSV 파일",
  PPT: "PowerPoint", PPTX: "PowerPoint",
  PDF: "PDF 문서", TXT: "텍스트 문서",
  JPG: "JPG 이미지", JPEG: "JPG 이미지", PNG: "PNG 이미지", GIF: "GIF 이미지", HEIC: "HEIC 이미지", WEBP: "WEBP 이미지",
  ZIP: "압축 파일", "7Z": "압축 파일", ALZ: "압축 파일", EGG: "압축 파일",
  MP4: "동영상", MOV: "동영상", MP3: "소리 파일", EXE: "프로그램",
};

/** 탐색기의 "유형"처럼: "PDF 문서", "한글 문서", 모르는 것은 "ZIP 파일" */
export function typeName(name: string): string {
  const ext = splitName(name)[1].slice(1).toUpperCase();
  if (!ext) return "파일";
  return TYPE_NAMES[ext] ?? `${ext} 파일`;
}
