import { useState } from "react";
import { Download, FolderOpen, Monitor, Plus } from "lucide-react";
import { errorMessage, pickFolder } from "../lib/backend";
import { friendlyName, samePath, type KnownFolders } from "../lib/paths";
import "./WatchFolderEditor.css";

interface Props {
  known: KnownFolders | null;
  value: string[];
  onChange: (folders: string[]) => void;
}

/** 지켜볼 폴더 고르기: 다운로드·바탕화면 + 직접 추가한 폴더. 처음 실행과 설정에서 같이 쓴다. */
export default function WatchFolderEditor({ known, value, onChange }: Props) {
  // 체크를 풀어도 목록에서 사라지지 않게, 한 번 보인 폴더는 기억해 둔다.
  const [shown, setShown] = useState<string[]>(() =>
    unique([known?.downloads, known?.desktop, ...value]),
  );
  const [error, setError] = useState<string | null>(null);

  const isOn = (path: string) => value.some((v) => samePath(v, path));

  const toggle = (path: string) => {
    onChange(isOn(path) ? value.filter((v) => !samePath(v, path)) : [...value, path]);
  };

  const add = async () => {
    setError(null);
    try {
      const picked = await pickFolder("지켜볼 폴더 고르기");
      if (!picked) return;
      setShown((list) => unique([...list, picked]));
      if (!isOn(picked)) onChange([...value, picked]);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <div className="watch">
      <ul className="watch__list">
        {shown.map((path) => {
          const name = friendlyName(path, known);
          const Icon = name === "다운로드" ? Download : name === "바탕화면" ? Monitor : FolderOpen;
          return (
            <li key={path}>
              <label className={`watch__row${isOn(path) ? " watch__row--on" : ""}`}>
                <input type="checkbox" checked={isOn(path)} onChange={() => toggle(path)} />
                <Icon size={18} className="watch__icon" aria-hidden="true" />
                <span className="watch__text">
                  <span className="watch__name">{name}</span>
                  <span className="watch__path">{path}</span>
                </span>
              </label>
            </li>
          );
        })}
      </ul>
      <div>
        <button type="button" className="btn" onClick={add}>
          <Plus size={16} aria-hidden="true" />
          다른 폴더 추가
        </button>
      </div>
      {error && <p className="error-text">{error}</p>}
    </div>
  );
}

function unique(paths: (string | null | undefined)[]): string[] {
  const out: string[] = [];
  for (const p of paths) {
    if (p && !out.some((o) => samePath(o, p))) out.push(p);
  }
  return out;
}
