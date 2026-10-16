import { useEffect, useState } from "react";
import { useDroppable } from "@dnd-kit/core";
import { FolderPlus } from "lucide-react";
import { createFolder, errorMessage, listFolders, onBackendEvent } from "../lib/backend";
import { baseName, trailFor, type Crumb } from "../lib/paths";
import type { FolderEntry } from "../lib/types";
import FolderIcon from "./FolderIcon";
import PathBar from "./PathBar";
import "./InboxFolderPane.css";

export interface Toast {
  message: string;
  /** 되돌릴 수 있는 이동들(없으면 되돌리기 버튼 없음) */
  moveIds: number[];
}

interface Props {
  root: string;
  separator: string;
  /** 선택한 파일 수(있으면 폴더마다 "여기에 넣기") */
  selectedCount: number;
  dragging: boolean;
  favorites: string[];
  onPut: (destDir: string) => void;
  /** 이 값이 바뀌면 그 폴더를 연다(자주 쓰는 폴더 누를 때) */
  jumpTo: { path: string; n: number } | null;
  onJump: (path: string) => void;
  toast: Toast | null;
  onUndo: () => void;
  busy: boolean;
}

/** 놓을 곳을 알려 주는 데이터(dnd-kit droppable의 data) */
export interface DropData {
  path: string;
}

/**
 * 정리함 오른쪽 폴더 칸(SPEC 3-2): 경로 + 자주 쓰는 폴더(숫자 키) + 폴더 아이콘 격자(누르면 안으로).
 * 파일을 끌어 폴더 위에 놓거나, 파일을 고른 뒤 "여기에 넣기"를 누르면 그 폴더에 들어간다.
 */
export default function InboxFolderPane(props: Props) {
  const { root, separator, selectedCount, dragging, favorites, onPut } = props;
  const [trail, setTrail] = useState<Crumb[]>([{ name: baseName(root), path: root }]);
  const [folders, setFolders] = useState<FolderEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");

  const current = trail[trail.length - 1];

  // 파일을 넣거나 되돌리면 폴더 안 파일 수가 바뀌므로 다시 읽는다.
  useEffect(() => onBackendEvent("inbox-changed", () => setReloadKey((k) => k + 1)), []);

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

  useEffect(() => {
    if (!props.jumpTo) return;
    const t = trailFor(root, props.jumpTo.path, separator);
    if (t) setTrail(t);
  }, [props.jumpTo, root, separator]);

  const make = async () => {
    try {
      await createFolder(current.path, newName);
      setCreating(false);
      setNewName("");
      setReloadKey((k) => k + 1);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <section className="ifold" aria-label="정리 폴더">
      <div className="ifold__bar">
        <PathBar trail={trail} onGo={(i) => setTrail((t) => t.slice(0, i + 1))}>
          {selectedCount > 0 && trail.length > 1 && (
            <button
              type="button"
              className="btn btn--accent ifold__here"
              onClick={() => onPut(current.path)}
              disabled={props.busy}
            >
              이 폴더에 넣기
            </button>
          )}
        </PathBar>
      </div>

      {favorites.length > 0 && (
        <div className="ifold__favs">
          <span className="ifold__favs-label">자주 쓰는 폴더</span>
          {favorites.map((path, i) => (
            <FavoriteChip
              key={path}
              path={path}
              index={i + 1}
              dragging={dragging}
              onClick={() => props.onJump(path)}
            />
          ))}
        </div>
      )}

      {creating && (
        <div className="ifold__new button-row">
          <input
            autoFocus
            aria-label="새 폴더 이름"
            placeholder={`${current.name} 안에 만들 폴더 이름`}
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void make();
              if (e.key === "Escape") setCreating(false);
            }}
          />
          <button type="button" className="btn btn--accent" onClick={() => void make()}>
            만들기
          </button>
          <button type="button" className="btn btn--ghost" onClick={() => setCreating(false)}>
            취소
          </button>
        </div>
      )}

      {error && <p className="ifold__note error-text">{error}</p>}

      <ul className="ifold__tiles">
        {folders?.map((f) => (
          <FolderTile
            key={f.path}
            folder={f}
            selectedCount={selectedCount}
            dragging={dragging}
            busy={props.busy}
            onOpen={() => setTrail((t) => [...t, { name: f.name, path: f.path }])}
            onPut={() => onPut(f.path)}
          />
        ))}
        <li>
          <button type="button" className="ifold__add" onClick={() => setCreating(true)}>
            <FolderPlus size={20} aria-hidden="true" />
            <span>새 폴더</span>
          </button>
        </li>
      </ul>

      {folders && folders.length === 0 && (
        <p className="ifold__note">
          이 폴더 안에는 하위 폴더가 없어요.
          {selectedCount > 0 && " 위의 ‘이 폴더에 넣기’를 누르면 여기에 넣어요."}
        </p>
      )}

      {props.toast && (
        <div className="ifold__toast" role="status">
          <span className="ifold__toast-text">{props.toast.message}</span>
          {props.toast.moveIds.length > 0 && (
            <button type="button" className="ifold__undo" onClick={props.onUndo} disabled={props.busy}>
              되돌리기
            </button>
          )}
        </div>
      )}
    </section>
  );
}

function FolderTile({
  folder,
  selectedCount,
  dragging,
  busy,
  onOpen,
  onPut,
}: {
  folder: FolderEntry;
  selectedCount: number;
  dragging: boolean;
  busy: boolean;
  onOpen: () => void;
  onPut: () => void;
}) {
  const { setNodeRef, isOver } = useDroppable({
    id: `tile:${folder.path}`,
    data: { path: folder.path } satisfies DropData,
  });
  return (
    <li ref={setNodeRef} className={`ifold__tile${isOver ? " ifold__tile--target" : ""}`}>
      <button type="button" className="ifold__open" onClick={onOpen} aria-label={`${folder.name} 폴더 열기`}>
        <FolderIcon tone={isOver ? "blue" : "yellow"} />
        <span className="ifold__name" title={folder.name}>
          {folder.name}
        </span>
      </button>
      <div className="ifold__meta">
        {isOver ? (
          <span className="ifold__drop">놓으면 여기로</span>
        ) : selectedCount > 0 && !dragging ? (
          <button type="button" className="ifold__put" onClick={onPut} disabled={busy}>
            여기에 넣기
          </button>
        ) : (
          <span>{folderMeta(folder)}</span>
        )}
      </div>
    </li>
  );
}

function FavoriteChip({
  path,
  index,
  dragging,
  onClick,
}: {
  path: string;
  index: number;
  dragging: boolean;
  onClick: () => void;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: `fav:${path}`, data: { path } satisfies DropData });
  return (
    <button
      ref={setNodeRef}
      type="button"
      className={`ifold__fav${isOver ? " ifold__fav--target" : ""}${dragging ? " ifold__fav--drop" : ""}`}
      onClick={onClick}
      title={path}
    >
      <span className="ifold__key">{index}</span>
      <span>{baseName(path)}</span>
    </button>
  );
}

function folderMeta(f: FolderEntry): string {
  if (f.fileCount > 0) return `파일 ${f.fileCount}개`;
  if (f.folderCount > 0) return `폴더 ${f.folderCount}개`;
  return "비어 있음";
}
