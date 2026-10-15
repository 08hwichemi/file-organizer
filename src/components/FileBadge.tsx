import { fileBadge } from "../lib/format";

/** 형식 배지(HWP·PDF…) */
export default function FileBadge({ name, size = 44 }: { name: string; size?: number }) {
  const b = fileBadge(name);
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
        fontSize: size >= 40 ? 12 : 10.5,
        fontWeight: 700,
      }}
    >
      {b.label}
    </span>
  );
}
