interface Props {
  size?: number;
  /** 놓일 곳 표시 등 강조할 때 파란 폴더 */
  tone?: "yellow" | "blue";
}

// 시안(docs/design)의 폴더 아이콘. 탐색기처럼 보이게 노란 폴더를 쓴다.
export default function FolderIcon({ size = 64, tone = "yellow" }: Props) {
  const back = tone === "blue" ? "#4C78D8" : "var(--color-folder-back)";
  const front = tone === "blue" ? "#7FA2EA" : "var(--color-folder-front)";
  return (
    <svg aria-hidden="true" width={size} height={(size * 52) / 64} viewBox="0 0 64 52">
      <path d="M4 8a5 5 0 0 1 5-5h14l6 6h26a5 5 0 0 1 5 5v4H4z" fill={back} />
      <rect x="2" y="15" width="60" height="35" rx="5" fill={front} />
    </svg>
  );
}
