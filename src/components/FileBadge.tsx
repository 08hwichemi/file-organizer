import { fileBadge } from "../lib/format";

/**
 * 형식 배지(HWP·PDF…). 글자는 늘 한 줄: 네 글자(HWPX·XLSX·PPTX·DOCX)는 칸 크기에 맞춰 글씨를 줄인다
 * (작은 배지에서 "HWP / X"처럼 갈라지지 않게).
 */
export default function FileBadge({ name, size = 44 }: { name: string; size?: number }) {
  const b = fileBadge(name);
  const base = size >= 40 ? 12 : 10.5;
  // 글자 하나 폭 ≈ 글씨 크기의 0.66배, 좌우 여백 4px씩
  const fit = (size - 8) / (b.label.length * 0.66);
  return (
    <span
      aria-hidden="true"
      style={{
        width: size,
        height: size,
        flexShrink: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        borderRadius: size >= 40 ? 10 : 8,
        background: b.bg,
        color: b.fg,
        fontSize: Math.min(base, Math.max(8, Math.floor(fit * 2) / 2)),
        fontWeight: 700,
        letterSpacing: b.label.length >= 4 ? "-0.02em" : undefined,
        whiteSpace: "nowrap",
        overflow: "hidden",
      }}
    >
      {b.label}
    </span>
  );
}
