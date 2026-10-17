import { useEffect, useMemo, useRef, useState } from "react";
import { useDraggable } from "@dnd-kit/core";
import { ExternalLink, FolderOpen } from "lucide-react";
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
  /** 함께 옮길 파일(고른 파일을 잡았으면 고른 것 모두) */
  paths: string[];
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
 * 고르기는 자동 분류 창과 같다(사용자 요청): 누르면 골라지고 다시 누르면 풀리며, Shift를 누른 채 누르면 사이의 파일까지.
 * 고른 파일을 잡고 끌면 고른 것이 다 같이 옮겨진다. 하나만 골랐을 때 [위치] [열기].
 * 지우기·복사·이름 바꾸기는 탐색기에서.
 */
export default function FolderFileList({ dir, reloadKey, focus, dragging, onError }: Props) {
  const [files, setFiles] = useState<FileEntry[] | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  // Shift로 사이를 고를 때의 기준(마지막으로 그냥 누른 파일)
  const anchor = useRef<string | null>(null);
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

  // 다른 폴더로 가거나 옮겨져 없어진 파일은 고른 것에서 뺀다(최근 정리에서 눌러 그 파일의 폴더로 온 때는 남긴다)
  useEffect(() => {
    const inDir = (path: string) => samePath(path.slice(0, Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"))), dir);
    setPicked((p) => new Set([...p].filter(inDir)));
  }, [dir]);
  useEffect(() => {
    if (!files) return;
    setPicked((p) => new Set([...p].filter((x) => files.some((f) => samePath(f.path, x)))));
  }, [files]);
  useEffect(() => {
    if (focus) {
      setPicked(new Set([focus.path]));
      anchor.current = focus.path;
    }
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

  if (!files) return null;
  const shown = sorted.slice(0, SHOW_MAX);
  const isPicked = (path: string) => [...picked].some((p) => samePath(p, path));

  const pick = (path: string, shift: boolean) => {
    const order = shown.map((f) => f.path);
    const from = anchor.current ? order.findIndex((p) => samePath(p, anchor.current ?? "")) : -1;
    const to = order.indexOf(path);
    if (shift && from >= 0 && to >= 0) {
      setPicked((p) => new Set([...p, ...order.slice(Math.min(from, to), Math.max(from, to) + 1)]));
      return;
    }
    anchor.current = path;
    setPicked((p) => {
      const next = new Set(p);
      const had = [...next].find((x) => samePath(x, path));
      if (had) next.delete(had);
      else next.add(path);
      return next;
    });
  };
  const clearPicked = () => {
    setPicked(new Set());
    anchor.current = null;
  };

  return (
    <>
      <div className="ffiles__head">
        <span className="ffiles__title">
          이 폴더의 파일 <span className="ffiles__count">{files.length}</span>
        </span>
        {picked.size > 0 ? (
          <span className="ffiles__sel">
            <b>{picked.size}개 고름</b>
            <button type="button" onClick={clearPicked}>
              고르기 풀기
            </button>
          </span>
        ) : (
          files.length > 1 && (
          <div className="ffiles__sort" role="group" aria-label="정렬">
            <button type="button" aria-pressed={sort === "recent"} onClick={() => setSort("recent")}>
              최근순
            </button>
            <button type="button" aria-pressed={sort === "name"} onClick={() => setSort("name")}>
              이름순
            </button>
          </div>
          )
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

      <section className={`ffiles${files.length === 0 ? " ffiles--empty" : ""}`} aria-label="이 폴더의 파일">
      {failed && <p className="ffiles__note error-text">{failed}</p>}
      {!failed && files.length === 0 && <p className="ffiles__note">이 폴더에는 파일이 없어요.</p>}

      {shown.length > 0 && (
        <ul className="ffiles__list" ref={listRef}>
          {shown.map((f) => (
            <FileRow
              key={f.path}
              file={f}
              picked={isPicked(f.path)}
              // [위치] [열기]는 하나만 골랐을 때
              only={picked.size === 1 && isPicked(f.path)}
              group={isPicked(f.path) ? [...picked] : [f.path]}
              dimmed={!!dragging?.some((p) => samePath(p, f.path))}
              onPick={(shift) => pick(f.path, shift)}
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
  only,
  group,
  dimmed,
  onPick,
  onOpen,
  onReveal,
}: {
  file: FileEntry;
  picked: boolean;
  only: boolean;
  /** 이 줄을 잡고 끌면 함께 옮길 파일 */
  group: string[];
  dimmed: boolean;
  /** shift = Shift를 누른 채 눌렀는지(사이의 파일까지 고르기) */
  onPick: (shift: boolean) => void;
  onOpen: () => void;
  onReveal: () => void;
}) {
  const { attributes, listeners, setNodeRef } = useDraggable({
    id: `ffile:${file.path}`,
    data: { file, paths: group } satisfies FileDragData,
  });
  return (
    <li
      ref={setNodeRef}
      data-path={file.path}
      className={`ffiles__row${picked ? " ffiles__row--on" : ""}${dimmed ? " ffiles__row--drag" : ""}`}
      {...attributes}
      {...listeners}
      onClick={(e) => onPick(e.shiftKey)}
      onDoubleClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          onOpen();
        } else if (e.key === " ") {
          e.preventDefault();
          onPick(e.shiftKey);
        }
      }}
      aria-pressed={picked}
      aria-label={`${file.name} · 누르면 고르기, 두 번 누르면 열기, 끌어서 다른 폴더로 옮기기`}
      title={`${file.name}\n누르면 골라지고(Shift: 사이의 파일까지), 두 번 누르면 열려요. 다른 폴더로 끌어다 놓으면 옮겨져요.`}
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
      {only && (
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
