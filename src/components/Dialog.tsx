import { useEffect, type ReactNode } from "react";
import "./Dialog.css";

interface Props {
  title: string;
  onClose: () => void;
  children: ReactNode;
  /** 아래 버튼 줄 */
  actions: ReactNode;
  wide?: boolean;
  /** 크기 등을 따로 정할 때 */
  className?: string;
}

/** 화면 가운데 작은 창. Esc나 바깥을 누르면 닫힌다. */
export default function Dialog({ title, onClose, children, actions, wide, className }: Props) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  return (
    <div className="dialog-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className={`dialog${wide ? " dialog--wide" : ""}${className ? ` ${className}` : ""}`} role="dialog" aria-modal="true" aria-label={title}>
        <h2 className="dialog__title">{title}</h2>
        <div className="dialog__body">{children}</div>
        <div className="dialog__actions button-row">{actions}</div>
      </div>
    </div>
  );
}
