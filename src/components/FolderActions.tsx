import { useState } from "react";
import { errorMessage, moveFolder, renameFolder, restoreDeleted, trashFolder } from "../lib/backend";
import { baseName, displayPath, parentPath } from "../lib/paths";
import type { FolderEntry } from "../lib/types";
import Dialog from "./Dialog";
import FolderPicker from "./FolderPicker";

export type FolderAction = "rename" | "move" | "trash";

export interface ActionDone {
  message: string;
  /** 되돌리기(있으면). 끝나면 보여 줄 문장을 돌려준다. */
  undo?: () => Promise<string>;
}

interface Props {
  action: FolderAction;
  folder: FolderEntry;
  root: string;
  separator: string;
  onClose: () => void;
  onDone: (result: ActionDone) => void;
}

/** 폴더 이름 바꾸기 · 다른 폴더로 옮기기 · 휴지통으로 보내기 창. 모두 되돌릴 수 있다. */
export default function FolderActions({ action, folder, root, separator, onClose, onDone }: Props) {
  const [name, setName] = useState(folder.name);
  const [dest, setDest] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (work: () => Promise<ActionDone>) => {
    setBusy(true);
    setError(null);
    try {
      onDone(await work());
      onClose();
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  };

  if (action === "rename") {
    const submit = () =>
      run(async () => {
        const renamed = await renameFolder(folder.path, name);
        return {
          message: `‘${folder.name}’ 폴더 이름을 ‘${renamed.name}’(으)로 바꿨어요`,
          undo: async () => {
            await renameFolder(renamed.path, folder.name);
            return `이름을 ‘${folder.name}’(으)로 되돌렸어요`;
          },
        };
      });
    return (
      <Dialog
        title="폴더 이름 바꾸기"
        onClose={onClose}
        actions={
          <>
            <button type="button" className="btn btn--ghost" onClick={onClose}>
              취소
            </button>
            <button
              type="button"
              className="btn btn--primary"
              onClick={() => void submit()}
              disabled={busy || !name.trim() || name.trim() === folder.name}
            >
              바꾸기
            </button>
          </>
        }
      >
        <input
          type="text"
          autoFocus
          aria-label="새 폴더 이름"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onFocus={(e) => e.target.select()}
          onKeyDown={(e) => e.key === "Enter" && void submit()}
        />
        {error && <p className="error-text">{error}</p>}
      </Dialog>
    );
  }

  if (action === "move") {
    const fromParent = parentPath(folder.path, separator);
    const submit = () =>
      run(async () => {
        if (!dest) throw "옮겨 넣을 폴더를 골라 주세요.";
        const moved = await moveFolder(folder.path, dest);
        return {
          message: `‘${folder.name}’ 폴더를 ${displayPath(root, dest, separator)} 안으로 옮겼어요`,
          undo: async () => {
            await moveFolder(moved.path, fromParent);
            return `‘${folder.name}’ 폴더를 원래 자리로 되돌렸어요`;
          },
        };
      });
    return (
      <Dialog
        wide
        title={`‘${folder.name}’ 폴더를 어디로 옮길까요?`}
        onClose={onClose}
        actions={
          <>
            <button type="button" className="btn btn--ghost" onClick={onClose}>
              취소
            </button>
            <button type="button" className="btn btn--primary" onClick={() => void submit()} disabled={busy || !dest}>
              여기로 옮기기
            </button>
          </>
        }
      >
        <p className="folder-move__target">
          옮길 곳 <strong>{dest ? displayPath(root, dest, separator) : "아래에서 폴더를 골라 주세요"}</strong>
        </p>
        <button
          type="button"
          className={`folder-move__root${dest === root ? " folder-move__root--on" : ""}`}
          onClick={() => setDest(root)}
        >
          맨 위({baseName(root)})로 옮기기
        </button>
        <FolderPicker root={root} separator={separator} selected={dest} onSelect={setDest} reveal={null} />
        {error && <p className="error-text">{error}</p>}
      </Dialog>
    );
  }

  const hasFiles = folder.fileCount > 0 || folder.folderCount > 0;
  const submit = () =>
    run(async () => {
      const id = await trashFolder(folder.path);
      return {
        message: `‘${folder.name}’ 폴더를 휴지통으로 보냈어요`,
        undo: async () => {
          const r = await restoreDeleted([id]);
          if (r.errors.length > 0) throw r.errors[0];
          return `‘${folder.name}’ 폴더를 되돌렸어요`;
        },
      };
    });
  return (
    <Dialog
      title={`‘${folder.name}’ 폴더를 휴지통으로 보낼까요?`}
      onClose={onClose}
      actions={
        <>
          <button type="button" className="btn btn--ghost" onClick={onClose}>
            취소
          </button>
          <button type="button" className="btn btn--danger" onClick={() => void submit()} disabled={busy} autoFocus>
            휴지통으로 보내기
          </button>
        </>
      }
    >
      <p>
        {hasFiles
          ? `안에 있는 ${[
              folder.folderCount > 0 ? `폴더 ${folder.folderCount}개` : "",
              folder.fileCount > 0 ? `파일 ${folder.fileCount}개` : "",
            ]
              .filter(Boolean)
              .join(", ")}도 함께 휴지통으로 가요.`
          : "빈 폴더예요."}
      </p>
      <p className="dialog__sub">바로 아래 ‘되돌리기’나 Windows 휴지통에서 다시 살릴 수 있어요.</p>
      {error && <p className="error-text">{error}</p>}
    </Dialog>
  );
}
