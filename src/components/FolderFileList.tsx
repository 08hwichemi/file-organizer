import { useEffect, useMemo, useRef, useState } from "react";
import { useDraggable } from "@dnd-kit/core";
import { ChevronDown, ExternalLink, FolderOpen } from "lucide-react";
import { errorMessage, listFiles, openPath, revealPath } from "../lib/backend";
import { formatDate, formatSize } from "../lib/format";
import { samePath } from "../lib/paths";
import type { FileEntry } from "../lib/types";
import FileBadge from "./FileBadge";
import "./FolderFileList.css";

/** 한 번에 그리는 파일 수. 더 많으면 탐색기에서 보도록 안내한다. */
const SHOW_MAX = 300;

type Sort = "recent" | "name";
const SORT_KEY = "folder-files-sort";

function savedSort(): Sort {
  try {
    return window.localStorage.getItem(SORT_KEY) === "name" ? "name" : "recent";
  } catch {
    return "recent";
  }
}

const byName = new Intl.Collator("ko", { numeric: true, sensitivity: "base" });

/** 끌고 있는 폴더 안 파일(dnd-kit draggable의 data) */
export interface FileDragData {
  file: FileEntry;
}

interface Props {
  /** 지금 보고 있는 폴더 */
  dir: string;
  /** 바뀌면 다시 읽는다(넣기·되돌리기·폴더 변경) */
  reloadKey: number;
  /** 이 파일을 골라 두고 보이게(최근 정리에서 눌렀을 때, n이 바뀔 때마다) */
  focus: { path: string; n: number } | null;
  /** 지금 끌고 있는 파일(흐리게) */
  dragging: string[] | null;
  onError: (message: string) => void;
}

/**
 * 폴더 칸에서 지금 폴더 바로 안의 파일(SPEC 3-2). 들여다보기만 한다:
 * 두 번 누르면 열기, [위치]는 탐색기에서 골라 두기, 끌어서 다른 폴더에 놓으면 옮기기(기록·되돌리기).
 * 지우기·복사·이름 바꾸기는 탐색기에서.
 */
export default function FolderFileList({ dir, reloadKey, focus, dragging, onError }: Props) {
  const [files, setFiles] = useState<FileEntry[] | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  const [sort, setSortState] = useState<Sort>(savedSort);
  const setSort = (s: Sort) => {
    setSortState(s);
    try {
      window.localStorage.setItem(SORT_KEY, s);
    } catch {
      // 기억하지 못해도 정렬은 바뀐다
    }
  };

  useEffect(() => {
    let alive = true;
    setFailed(null);
    listFiles(dir)
      .then((list) => alive && setFiles(list))
      .catch((e) => {
        if (!alive) return;
        setFiles([]);
        setFailed(errorMessage(e));
      });
    return () => {
      alive = false;
    };
  }, [dir, reloadKey]);

  // 다른 폴더로 가면 고른 것을 지운다(최근 정리에서 눌러 그 파일의 폴더로 온 때는 남긴다)
  useEffect(() => {
    const inDir = (path: string) => samePath(path.slice(0, Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"))), dir);
    setPicked((p) => (p && inDir(p) ? p : null));
  }, [dir]);
  useEffect(() => {
    if (focus) setPicked(focus.path);
  }, [focus]);

  const sorted = useMemo(() => {
    if (!files) return [];
    const list = [...files];
    if (sort === "name") list.sort((a, b) => byName.compare(a.name, b.name));
    else list.sort((a, b) => b.modified - a.modified || byName.compare(a.name, b.name));
    return list;
  }, [files, sort]);

  // 고른 파일이 목록 아래쪽에 있어도 보이게
  const listRef = useRef<HTMLUListElement>(null);
  useEffect(() => {
    if (!focus || !files) return;
    const row = [...(listRef.current?.querySelectorAll<HTMLElement>("[data-path]") ?? [])].find((el) =>
      samePath(el.dataset.path ?? "", focus.path),
    );
    row?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [focus, files]);

  const run = (what: Promise<void>) => void what.catch((e) => onError(errorMessage(e)));

  // 제목 줄은 폴더 칸 맨 아래에 붙어 있다(폴더가 많아 파일이 아래로 밀려도 있다는 걸 알게). 누르면 파일 목록으로.
  const headRef = useRef<HTMLDivElement>(null);
  const jump = () => headRef.current?.scrollIntoView({ block: "start", behavior: "smooth" });

  if (!files) return null;
  const shown = sorted.slice(0, SHOW_MAX);

  return (
    <>
      <div className="ffiles__head" ref={headRef}>
        <button
          type="button"
          className="ffiles__title"
          onClick={jump}
          disabled={files.length === 0}
          title="이 폴더의 파일 목록으로 내려가요"
        >
          이 폴더의 파일 <span className="ffiles__count">{files.length}</span>
          {files.length > 0 && <ChevronDown size={14} aria-hidden="true" />}
        </button>
        {files.length > 1 && (
          <div className="ffiles__sort" role="group" aria-label="정렬">
            <button type="button" aria-pressed={sort === "recent"} onClick={() => setSort("recent")}>
              최근순
            </button>
            <button type="button" aria-pressed={sort === "name"} onClick={() => setSort("name")}>
              이름순
            </button>
          </div>
        )}
        <span className="ffiles__spacer" />
        <button
          type="button"
          className="ffiles__explorer"
          onClick={() => run(openPath(dir))}
          title="이 폴더를 Windows 탐색기에서 열어요(지우기·복사·이름 바꾸기는 탐색기에서)"
        >
          <FolderOpen size={14} aria-hidden="true" />
          탐색기에서 열기
        </button>
      </div>

      <section className="ffiles" aria-label="이 폴더의 파일">
      {failed && <p className="ffiles__note error-text">{failed}</p>}
      {!failed && files.length === 0 && <p className="ffiles__note">이 폴더에는 파일이 없어요.</p>}

      {shown.length > 0 && (
        <ul className="ffiles__list" ref={listRef}>
          {shown.map((f) => (
            <FileRow
              key={f.path}
              file={f}
              picked={picked !== null && samePath(picked, f.path)}
              dimmed={!!dragging?.some((p) => samePath(p, f.path))}
              onPick={() => setPicked(f.path)}
              onOpen={() => run(openPath(f.path))}
              onReveal={() => run(revealPath(f.path))}
            />
          ))}
        </ul>
      )}
      {files.length > SHOW_MAX && (
        <p className="ffiles__note">
          {SHOW_MAX}개까지 보여 드려요. 나머지 {files.length - SHOW_MAX}개는 탐색기에서 보세요.
        </p>
      )}
      </section>
    </>
  );
}

function FileRow({
  file,
  picked,
  dimmed,
  onPick,
  onOpen,
  onReveal,
}: {
  file: FileEntry;
  picked: boolean;
  dimmed: boolean;
  onPick: () => void;
  onOpen: () => void;
  onReveal: () => void;
}) {
  const { attributes, listeners, setNodeRef } = useDraggable({
    id: `ffile:${file.path}`,
    data: { file } satisfies FileDragData,
  });
  return (
    <li
      ref={setNodeRef}
      data-path={file.path}
      className={`ffiles__row${picked ? " ffiles__row--on" : ""}${dimmed ? " ffiles__row--drag" : ""}`}
      {...attributes}
      {...listeners}
      onClick={onPick}
      onDoubleClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          onOpen();
        } else if (e.key === " ") {
          e.preventDefault();
          onPick();
        }
      }}
      aria-pressed={picked}
      aria-label={`${file.name} · 두 번 누르면 열기, 끌어서 다른 폴더로 옮기기`}
      title={`${file.name}\n두 번 누르면 열려요. 다른 폴더로 끌어다 놓으면 옮겨져요.`}
    >
      <FileBadge name={file.name} size={30} />
      <div className="ffiles__text">
        <span className="ffiles__name">{file.name}</span>
        <span className="ffiles__meta">
          {file.modified > 0 ? formatDate(file.modified) : ""}
          {file.modified > 0 ? " · " : ""}
          {formatSize(file.size)}
        </span>
      </div>
      {picked && (
        <div className="ffiles__acts">
          <button
            type="button"
            className="ffiles__btn"
            onClick={(e) => {
              e.stopPropagation();
              onReveal();
            }}
            onPointerDown={(e) => e.stopPropagation()}
            title="탐색기에서 이 파일이 있는 곳을 열어요"
          >
            위치
          </button>
          <button
            type="button"
            className="ffiles__btn ffiles__btn--main"
            onClick={(e) => {
              e.stopPropagation();
              onOpen();
            }}
            onPointerDown={(e) => e.stopPropagation()}
          >
            <ExternalLink size={13} aria-hidden="true" />
            열기
          </button>
        </div>
      )}
    </li>
  );
}
