import type { ReactNode } from "react";
import { ChevronLeft } from "lucide-react";
import type { Crumb } from "../lib/paths";
import "./PathBar.css";

interface Props {
  trail: Crumb[];
  onGo: (index: number) => void;
  /** 오른쪽 끝에 붙일 버튼 등 */
  children?: ReactNode;
  compact?: boolean;
}

/** ‹ 버튼 + 경로(정리 폴더 › 평가 › 수행평가). 경로 조각을 누르면 그 폴더로. */
export default function PathBar({ trail, onGo, children, compact }: Props) {
  return (
    <div className={`pathbar${compact ? " pathbar--compact" : ""}`}>
      <button
        type="button"
        className="icon-btn"
        aria-label="위 폴더로"
        disabled={trail.length <= 1}
        onClick={() => onGo(trail.length - 2)}
      >
        <ChevronLeft size={18} />
      </button>
      <nav className="pathbar__crumbs" aria-label="현재 위치">
        {trail.map((c, i) => (
          <span key={c.path} className="pathbar__crumb">
            {i > 0 && (
              <span className="pathbar__sep" aria-hidden="true">
                ›
              </span>
            )}
            <button
              type="button"
              className={i === trail.length - 1 ? "pathbar__btn pathbar__btn--here" : "pathbar__btn"}
              aria-current={i === trail.length - 1 ? "location" : undefined}
              onClick={() => onGo(i)}
            >
              {c.name}
            </button>
          </span>
        ))}
      </nav>
      {children}
    </div>
  );
}
