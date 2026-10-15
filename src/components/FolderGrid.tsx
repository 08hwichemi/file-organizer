import { useEffect, useState } from "react";
import { errorMessage, listFolders, onBackendEvent } from "../lib/backend";
import { baseName, type Crumb } from "../lib/paths";
import type { FolderEntry } from "../lib/types";
import FolderIcon from "./FolderIcon";
import PathBar from "./PathBar";
import "./FolderGrid.css";

interface Props {
  rootPath: string;
}

/**
 * 탐색기처럼 폴더 아이콘 격자. 폴더를 누르면 안으로, ‹ 버튼이나 경로를 누르면 위로.
 * 정리 폴더가 바뀌면 처음부터 보이도록 부르는 쪽에서 key={rootPath}를 준다.
 */
export default function FolderGrid({ rootPath }: Props) {
  const [trail, setTrail] = useState<Crumb[]>([{ name: baseName(rootPath), path: rootPath }]);
  const [folders, setFolders] = useState<FolderEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  // 파일을 넣거나 되돌리면 폴더 안 파일 수가 바뀌므로 다시 읽는다.
  useEffect(() => onBackendEvent("inbox-changed", () => setReloadKey((k) => k + 1)), []);

  const current = trail[trail.length - 1];

  useEffect(() => {
    let alive = true;
    setError(null);
    listFolders(current.path)
      .then((list) => alive && setFolders(list))
      .catch((e) => alive && setError(errorMessage(e)));
    return () => {
      alive = false;
    };
  }, [current.path, reloadKey]);

  const open = (f: FolderEntry) => setTrail((t) => [...t, { name: f.name, path: f.path }]);
  const goTo = (index: number) => setTrail((t) => t.slice(0, index + 1));

  return (
    <section className="fgrid" aria-label="정리 폴더">
      <div className="fgrid__bar">
        <PathBar trail={trail} onGo={goTo} />
      </div>

      {error && <p className="fgrid__note error-text">{error}</p>}
      {!error && folders && folders.length === 0 && (
        <p className="fgrid__note">이 폴더 안에는 하위 폴더가 없어요.</p>
      )}
      {!error && folders && folders.length > 0 && (
        <ul className="fgrid__tiles">
          {folders.map((f) => (
            <li key={f.path}>
              <button
                type="button"
                className="fgrid__tile"
                onClick={() => open(f)}
                aria-label={`${f.name} 폴더 열기`}
              >
                <FolderIcon />
                <span className="fgrid__name" title={f.name}>
                  {f.name}
                </span>
                <span className="fgrid__meta">{folderMeta(f)}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function folderMeta(f: FolderEntry): string {
  if (f.fileCount > 0) return `파일 ${f.fileCount}개`;
  if (f.folderCount > 0) return `폴더 ${f.folderCount}개`;
  return "비어 있음";
}
