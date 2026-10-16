import { Plus, X } from "lucide-react";
import type { FolderSpec } from "../lib/types";
import FolderIcon from "./FolderIcon";
import "./StructureEditor.css";

interface Props {
  value: FolderSpec[];
  onChange: (value: FolderSpec[]) => void;
}

/** 처음 설정: 만들 폴더를 직접 고치기(이름 바꾸기 · 더하기 · 빼기). 맨 위 폴더와 그 안의 폴더 두 단계. */
export default function StructureEditor({ value, onChange }: Props) {
  const update = (i: number, next: FolderSpec) => onChange(value.map((f, j) => (j === i ? next : f)));
  const remove = (i: number) => onChange(value.filter((_, j) => j !== i));

  return (
    <div className="structure">
      <ul className="structure__list">
        {value.map((folder, i) => (
          <li key={i} className="structure__item">
            <div className="structure__row">
              <FolderIcon size={26} />
              <input
                className="structure__name"
                aria-label={`${i + 1}번째 폴더 이름`}
                value={folder.name}
                placeholder="폴더 이름"
                onChange={(e) => update(i, { ...folder, name: e.target.value })}
              />
              <button
                type="button"
                className="structure__add-sub"
                onClick={() => update(i, { ...folder, children: [...folder.children, { name: "", children: [] }] })}
              >
                <Plus size={14} aria-hidden="true" />안에 폴더
              </button>
              <button
                type="button"
                className="structure__remove"
                aria-label={`‘${folder.name || "이름 없는"}’ 폴더 빼기`}
                onClick={() => remove(i)}
              >
                <X size={16} />
              </button>
            </div>
            {folder.children.length > 0 && (
              <div className="structure__children">
                {folder.children.map((child, k) => (
                  <span key={k} className="structure__chip">
                    <input
                      aria-label={`${folder.name} 안 ${k + 1}번째 폴더 이름`}
                      value={child.name}
                      placeholder="폴더 이름"
                      size={Math.max(4, child.name.length + 1)}
                      autoFocus={!child.name}
                      onChange={(e) =>
                        update(i, {
                          ...folder,
                          children: folder.children.map((c, m) => (m === k ? { ...c, name: e.target.value } : c)),
                        })
                      }
                    />
                    <button
                      type="button"
                      aria-label={`‘${child.name || "이름 없는"}’ 폴더 빼기`}
                      onClick={() => update(i, { ...folder, children: folder.children.filter((_, m) => m !== k) })}
                    >
                      <X size={13} />
                    </button>
                  </span>
                ))}
              </div>
            )}
          </li>
        ))}
      </ul>
      <button
        type="button"
        className="btn structure__add"
        onClick={() => onChange([...value, { name: "", children: [] }])}
      >
        <Plus size={16} aria-hidden="true" />
        폴더 추가
      </button>
    </div>
  );
}

/** 문제가 있으면 사용자에게 보여 줄 문장(빈 이름, 같은 자리 같은 이름, 쓸 수 없는 글자). */
export function structureProblem(value: FolderSpec[]): string | null {
  const check = (list: FolderSpec[]): string | null => {
    const seen = new Set<string>();
    for (const f of list) {
      const name = f.name.trim();
      if (!name) return "이름이 비어 있는 폴더가 있어요.";
      if (/[\\/:*?"<>|]/.test(name)) return `‘${name}’에 쓸 수 없는 글자가 있어요(\\ / : * ? " < > |).`;
      if (seen.has(name.toLowerCase())) return `같은 자리에 ‘${name}’ 폴더가 두 번 있어요.`;
      seen.add(name.toLowerCase());
      const inner = check(f.children);
      if (inner) return inner;
    }
    return null;
  };
  return check(value);
}
