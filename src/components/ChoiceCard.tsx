import type { ReactNode } from "react";
import "./ChoiceCard.css";

interface Props {
  name: string;
  checked: boolean;
  onSelect: () => void;
  icon?: ReactNode;
  title: string;
  description?: ReactNode;
  children?: ReactNode;
}

/** 큰 선택 카드(라디오 버튼). 카드 어디를 눌러도 선택된다. */
export default function ChoiceCard({ name, checked, onSelect, icon, title, description, children }: Props) {
  return (
    <label className={`choice${checked ? " choice--on" : ""}`}>
      <input
        type="radio"
        name={name}
        className="visually-hidden"
        checked={checked}
        onChange={onSelect}
        // 이미 선택된 카드를 다시 눌러도 동작하게(예: 다른 폴더 다시 고르기)
        onClick={checked ? onSelect : undefined}
      />
      {icon && <span className="choice__icon">{icon}</span>}
      <span className="choice__body">
        <span className="choice__title">{title}</span>
        {description && <span className="choice__desc">{description}</span>}
        {children}
      </span>
      <span className="choice__radio" aria-hidden="true" />
    </label>
  );
}
