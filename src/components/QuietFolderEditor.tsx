import { useState } from "react";
import { BellOff, Plus } from "lucide-react";
import { errorMessage, pickFolder } from "../lib/backend";
import { baseName, friendlyPlace, samePath, type KnownFolders } from "../lib/paths";
import "./WatchFolderEditor.css";

interface Props {
  known: KnownFolders | null;
  value: string[];
  onChange: (folders: string[]) => void;
}

/**
 * 묻지 않는 폴더(SPEC 5장): 여기(하위 폴더까지)에 새 파일이 생겨도 묻지 않는다.
 * 팝업 ⋯ "이 폴더는 묻지 않기"로도 더해진다. [다시 묻기]로 목록에서 뺀다.
 */
export default function QuietFolderEditor({ known, value, onChange }: Props) {
  const [error, setError] = useState<string | null>(null);

  const add = async () => {
    setError(null);
    try {
      const picked = await pickFolder("묻지 않을 폴더 고르기");
      if (picked && !value.some((v) => samePath(v, picked))) onChange([...value, picked]);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <div className="quiet">
      <h3 className="quiet__title">
        묻지 않는 폴더 {value.length > 0 && <span className="quiet__count">{value.length}</span>}
      </h3>
      <p className="quiet__desc">
        여기에 새 파일이 생기면 묻지 않아요. 팝업의 ⋯ 메뉴 ‘이 폴더는 묻지 않기’로도 더해져요.
      </p>
      <div className="watch">
        {value.length > 0 && (
          <ul className="watch__list">
            {value.map((path) => (
              <li key={path}>
                <div className="watch__row watch__row--static">
                  <BellOff size={18} className="watch__icon" aria-hidden="true" />
                  <span className="watch__text">
                    <span className="watch__name">{baseName(path)}</span>
                    <span className="watch__path" title={path}>
                      {friendlyPlace(path, known)}
                    </span>
                  </span>
                  <button
                    type="button"
                    className="btn quiet__undo"
                    onClick={() => onChange(value.filter((v) => !samePath(v, path)))}
                  >
                    다시 묻기
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
        <div>
          <button type="button" className="btn" onClick={add}>
            <Plus size={16} aria-hidden="true" />
            폴더 더하기
          </button>
        </div>
        {error && <p className="error-text">{error}</p>}
      </div>
    </div>
  );
}
