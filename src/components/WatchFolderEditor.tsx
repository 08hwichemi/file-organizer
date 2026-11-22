import { useState, type ReactNode } from "react";
import { Download, FolderOpen, MessageCircle, Monitor, Plus } from "lucide-react";
import { errorMessage, pickFolder } from "../lib/backend";
import { friendlyName, samePath, type KnownFolders } from "../lib/paths";
import "./WatchFolderEditor.css";

interface Props {
  known: KnownFolders | null;
  value: string[];
  onChange: (folders: string[]) => void;
  /** 목록 아래, [폴더 더하기] 위에 보일 안내 */
  note?: ReactNode;
}

/** 지켜볼 폴더(받은 파일이 모이는 폴더) 고르기: 다운로드·바탕화면·카카오톡 받은 파일 + 직접 더한 폴더.
 * 처음 실행과 설정에서 같이 쓴다. */
export default function WatchFolderEditor({ known, value, onChange, note }: Props) {
  // 체크를 풀어도 목록에서 사라지지 않게, 한 번 보인 폴더는 기억해 둔다.
  const [shown, setShown] = useState<string[]>(() =>
    unique([known?.downloads, known?.desktop, known?.kakao, ...value]),
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
          const Icon =
            name === "다운로드"
              ? Download
              : name === "바탕화면"
                ? Monitor
                : known?.kakao && samePath(path, known.kakao)
                  ? MessageCircle
                  : FolderOpen;
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
      {note && <p className="quiet__desc">{note}</p>}
      <div>
        <button type="button" className="btn" onClick={add}>
          <Plus size={16} aria-hidden="true" />
          폴더 더하기
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
