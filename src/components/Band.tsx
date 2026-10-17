import type { ReactNode } from "react";

/**
 * 칸 제목 띠(프로그램 전체에서 같은 모양). 색의 뜻도 어디서나 같다:
 * - blue = 지금 할 일·넣을 곳(넣을 파일, 추천 위치, 추천 위치가 있는 파일)
 * - gray = 고르거나 참고하는 곳(이미 있는 파일, 폴더, 최근 넣은 곳, 직접 골라야 하는 파일)
 */
export default function Band({
  tone,
  title,
  count,
  hint,
  className,
}: {
  tone: "blue" | "gray";
  title: ReactNode;
  /** 제목 옆 동그란 수 */
  count?: number;
  /** 오른쪽 끝의 짧은 안내 */
  hint?: ReactNode;
  className?: string;
}) {
  return (
    <div className={`band band--${tone}${className ? ` ${className}` : ""}`}>
      <span className="band__title">{title}</span>
      {count !== undefined && <b className="band__count">{count}</b>}
      {hint && <span className="band__hint">{hint}</span>}
    </div>
  );
}
