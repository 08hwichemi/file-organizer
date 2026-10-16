import { useCallback, useEffect, useMemo, useState } from "react";
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import ClassifyDialog from "../components/ClassifyDialog";
import FileBadge from "../components/FileBadge";
import InboxFileList, { ALL_TAB } from "../components/InboxFileList";
import FolderPane, { type DropData, type Toast } from "../components/FolderPane";
import {
  classifyFiles,
  errorMessage,
  getFavoriteFolders,
  importExisting,
  listInbox,
  onBackendEvent,
  putFiles,
  putSuggested,
  restoreDeleted,
  trashFiles,
  undoMoves,
} from "../lib/backend";
import { displayPath } from "../lib/paths";
import type { InboxItem } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import "./InboxPage.css";

// 정리함(SPEC 3-2). 넣는 방법 세 가지: 끌어다 놓기 · 체크 후 "여기에 넣기" · 체크 후 숫자 키(자주 쓰는 폴더).
export default function InboxPage() {
  const settings = useAppStore((s) => s.settings);
  const known = useAppStore((s) => s.setupInfo);
  const root = settings?.rootFolder ?? null;
  const sep = known?.separator ?? "\\";

  const [items, setItems] = useState<InboxItem[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [tab, setTab] = useState(ALL_TAB);
  const [favorites, setFavorites] = useState<string[]>([]);
  const [toast, setToast] = useState<Toast | null>(null);
  const [busy, setBusy] = useState(false);
  const [importing, setImporting] = useState(false);
  const [dragging, setDragging] = useState<string[] | null>(null);
  const [classifying, setClassifying] = useState<string[] | null>(null);
  const [jumpTo, setJumpTo] = useState<{ path: string; n: number } | null>(null);

  const reload = useCallback(() => {
    void listInbox()
      .then((list) => {
        setItems(list);
        // 없어진 파일은 선택에서 뺀다
        setSelected((sel) => new Set([...sel].filter((p) => list.some((i) => i.path === p))));
      })
      .catch(() => setItems([]));
    void getFavoriteFolders()
      .then(setFavorites)
      .catch(() => setFavorites([]));
  }, []);

  useEffect(() => {
    reload();
    return onBackendEvent("inbox-changed", reload);
  }, [reload]);

  // 알림은 잠시 뒤 사라진다
  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), 10_000);
    return () => window.clearTimeout(timer);
  }, [toast]);

  const nameOf = useCallback(
    (path: string) => items?.find((i) => i.path === path)?.name ?? path,
    [items],
  );

  const put = useCallback(
    async (paths: string[], destDir: string) => {
      if (paths.length === 0 || busy) return;
      setBusy(true);
      try {
        const outcomes = await putFiles(paths, destDir);
        const done = outcomes.filter((o) => o.moveId !== null);
        const failed = outcomes.filter((o) => o.error);
        const where = displayPath(root, destDir, sep);
        let message =
          done.length === 1
            ? `‘${nameOf(done[0].path)}’ 파일을 ${where}에 넣었어요`
            : done.length > 1
              ? `${done.length}개 파일을 ${where}에 넣었어요`
              : "넣지 못했어요";
        if (failed.length > 0) message += ` · ${failed.length}개는 못 넣었어요: ${failed[0].error}`;
        const moveIds = done.map((o) => o.moveId as number);
        setToast({
          message,
          undo:
            moveIds.length > 0
              ? async () => {
                  const r = await undoMoves(moveIds);
                  return r.errors.length === 0
                    ? `${r.restored}개 파일을 원래 자리로 되돌렸어요`
                    : `${r.restored}개를 되돌렸어요 · ${r.errors[0]}`;
                }
              : undefined,
        });
        setSelected((sel) => new Set([...sel].filter((p) => !done.some((o) => o.path === p))));
      } catch (e) {
        setToast({ message: errorMessage(e) });
      } finally {
        setBusy(false);
      }
    },
    [busy, nameOf, root, sep],
  );

  // 정해진 곳에 넣기: 줄의 [넣기](추천 위치) 또는 [자동 분류](미리보기에서 확인한 파일 + 직접 옮긴 파일)
  const accept = async (
    paths: string[],
    how: "suggest" | "classify",
    manual: { destDir: string; paths: string[] }[] = [],
  ) => {
    if (paths.length + manual.length === 0 || busy) return;
    setBusy(true);
    try {
      const outcomes =
        how === "suggest" ? await putSuggested(paths) : paths.length > 0 ? await classifyFiles(paths) : [];
      for (const m of manual) outcomes.push(...(await putFiles(m.paths, m.destDir)));
      const done = outcomes.filter((o) => o.moveId !== null);
      const failed = outcomes.filter((o) => o.error);
      const where = how === "suggest" ? "추천 위치에 넣었어요" : "자동으로 나눴어요";
      let message =
        done.length === 1
          ? `‘${nameOf(done[0].path)}’ 파일을 ${where}`
          : done.length > 1
            ? `${done.length}개 파일을 ${where}`
            : "넣지 못했어요";
      if (failed.length > 0) message += ` · ${failed.length}개는 못 넣었어요: ${failed[0].error}`;
      const moveIds = done.map((o) => o.moveId as number);
      setToast({
        message,
        undo:
          moveIds.length > 0
            ? async () => {
                const r = await undoMoves(moveIds);
                return r.errors.length === 0
                  ? `${r.restored}개 파일을 원래 자리로 되돌렸어요`
                  : `${r.restored}개를 되돌렸어요 · ${r.errors[0]}`;
              }
            : undefined,
      });
      setSelected((sel) => new Set([...sel].filter((p) => !done.some((o) => o.path === p))));
    } catch (e) {
      setToast({ message: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  };

  // 고른 파일을 휴지통으로(되돌리기 가능)
  const trashSelected = async () => {
    const paths = [...selected];
    if (paths.length === 0 || busy) return;
    setBusy(true);
    try {
      const r = await trashFiles(paths);
      const n = r.deletionIds.length;
      let message =
        n === 1 ? `‘${nameOf(paths[0])}’ 파일을 휴지통으로 보냈어요` : `${n}개 파일을 휴지통으로 보냈어요`;
      if (r.errors.length > 0) message += ` · ${r.errors.length}개는 못 보냈어요: ${r.errors[0]}`;
      setToast({
        message,
        undo:
          n > 0
            ? async () => {
                const u = await restoreDeleted(r.deletionIds);
                return u.errors.length === 0 ? `${u.restored}개 파일을 되돌렸어요` : `${u.restored}개를 되돌렸어요 · ${u.errors[0]}`;
              }
            : undefined,
      });
      setSelected(new Set());
    } catch (e) {
      setToast({ message: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  };

  const runImport = async () => {
    setImporting(true);
    try {
      const added = await importExisting();
      setToast({
        message: added > 0 ? `지금 있는 파일 ${added}개를 가져왔어요` : "새로 가져올 파일이 없어요",
      });
    } catch (e) {
      setToast({ message: errorMessage(e) });
    } finally {
      setImporting(false);
    }
  };

  // 숫자 키 1~6: 고른 파일을 자주 쓰는 폴더에 넣기. Esc: 선택 해제.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.target instanceof HTMLInputElement || e.ctrlKey || e.altKey || e.metaKey) return;
      if (e.key === "Escape") setSelected(new Set());
      const n = Number(e.key);
      if (n >= 1 && n <= favorites.length && selected.size > 0) {
        e.preventDefault();
        void put([...selected], favorites[n - 1]);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [favorites, put, selected]);

  // 끌어다 놓기: 조금(6px) 움직여야 끌기 시작 → 그냥 누르면 선택
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));

  const onDragStart = (e: DragStartEvent) => {
    const path = String(e.active.id);
    setDragging(selected.has(path) ? [...selected] : [path]);
  };

  const onDragEnd = (e: DragEndEvent) => {
    const target = e.over?.data.current as DropData | undefined;
    const paths = dragging ?? [];
    setDragging(null);
    if (target?.path) void put(paths, target.path);
  };

  const toggle = (path: string) =>
    setSelected((sel) => {
      const next = new Set(sel);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const selectMany = (paths: string[]) =>
    setSelected((sel) => (paths.every((p) => sel.has(p)) ? new Set() : new Set(paths)));

  const overlay = useMemo(() => {
    if (!dragging) return null;
    const first = nameOf(dragging[0]);
    return (
      <div className="inbox-drag">
        <FileBadge name={first} size={26} />
        <span>{dragging.length === 1 ? first : `${first} 외 ${dragging.length - 1}개`}</span>
      </div>
    );
  }, [dragging, nameOf]);

  return (
    <div className="page page--fill">
      <div>
        <h1 className="page__title">정리함</h1>
        <p className="page__lead">
          {settings?.organizeMode === "manual"
            ? "새 파일은 여기에 모여요. "
            : "‘나중에 정리’한 파일은 여기에 모여요. "}
          왼쪽 파일을 오른쪽 폴더 위로 끌어다 놓으면 들어가요. 폴더를 누르면 안으로 들어가요.
        </p>
      </div>
      <DndContext sensors={sensors} onDragStart={onDragStart} onDragEnd={onDragEnd} onDragCancel={() => setDragging(null)}>
        <div className="inbox">
          {items && (
            <InboxFileList
              items={items}
              watchFolders={settings?.watchFolders ?? []}
              known={known}
              tab={tab}
              onTab={setTab}
              selected={selected}
              onToggle={toggle}
              onSelectMany={selectMany}
              onClear={() => setSelected(new Set())}
              dragging={dragging}
              onImport={() => void runImport()}
              importing={importing}
              onTrash={() => void trashSelected()}
              onAccept={(paths) => void accept(paths, "suggest")}
              onClassify={setClassifying}
              root={root}
              separator={sep}
              busy={busy}
            />
          )}
          {root && (
            <FolderPane
              key={root}
              fill
              root={root}
              separator={sep}
              selectedCount={selected.size}
              dragging={!!dragging}
              favorites={favorites}
              onPut={(dest) => void put([...selected], dest)}
              jumpTo={jumpTo}
              onJump={(path) => setJumpTo((j) => ({ path, n: (j?.n ?? 0) + 1 }))}
              toast={toast}
              onToast={setToast}
              busy={busy}
            />
          )}
        </div>
        <DragOverlay dropAnimation={null}>{overlay}</DragOverlay>
      </DndContext>
      {classifying && root && (
        <ClassifyDialog
          paths={classifying}
          root={root}
          separator={sep}
          onClose={() => setClassifying(null)}
          onRun={(auto, manual) => {
            setClassifying(null);
            void accept(auto, "classify", manual);
          }}
        />
      )}
    </div>
  );
}
