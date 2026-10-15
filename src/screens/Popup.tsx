import { useCallback, useEffect, useRef, useState } from "react";
import { Check, FolderOpen, Pencil, X } from "lucide-react";
import FileBadge from "../components/FileBadge";
import FolderIcon from "../components/FolderIcon";
import FolderPicker from "../components/FolderPicker";
import {
  deferFile,
  errorMessage,
  getPopupState,
  getRecentDestinations,
  hidePopup,
  onBackendEvent,
  pickFolder,
  putFile,
  undoMove,
} from "../lib/backend";
import { formatSize, splitName } from "../lib/format";
import { baseName, displayPath, friendlyName } from "../lib/paths";
import type { PendingFile, PutResult } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import "./Popup.css";

type Phase = { kind: "pick" } | { kind: "done"; file: PendingFile; result: PutResult };

// 새 파일 팝업(SPEC 3-1). 오른쪽 아래 작은 창. Enter = 넣기, Esc = 나중에 정리, 1·2·3 = 최근 넣은 곳.
export default function Popup() {
  const settings = useAppStore((s) => s.settings);
  const info = useAppStore((s) => s.setupInfo);
  const reloadSettings = useAppStore((s) => s.load);
  const root = settings?.rootFolder ?? null;
  const sep = info?.separator ?? "\\";

  const [current, setCurrent] = useState<PendingFile | null>(null);
  const [remaining, setRemaining] = useState(0);
  const [phase, setPhase] = useState<Phase>({ kind: "pick" });
  const [selected, setSelected] = useState<string | null>(null);
  const [reveal, setReveal] = useState<string | null>(null);
  const [recents, setRecents] = useState<string[]>([]);
  const [newName, setNewName] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const shownPath = useRef<string | null>(null);
  const phaseKind = useRef<Phase["kind"]>("pick");
  phaseKind.current = phase.kind;

  const refresh = useCallback(async () => {
    // 팝업 창은 프로그램 시작 때 만들어지므로, 그 뒤 바뀐 설정(정리 폴더 등)을 다시 읽는다.
    await reloadSettings();
    const state = await getPopupState();
    setCurrent(state.current);
    setRemaining(state.remaining);
    // 다른 파일로 바뀌면 고른 것 초기화
    if (state.current?.path !== shownPath.current) {
      shownPath.current = state.current?.path ?? null;
      setSelected(null);
      setReveal(null);
      setNewName(null);
      setEditing(null);
      setError(null);
      setRecents(await getRecentDestinations().catch(() => []));
    }
    return state;
  }, [reloadSettings]);

  useEffect(() => {
    void refresh();
    return onBackendEvent("popup-changed", () => {
      // 정리 결과를 보여 주는 중에는 남은 개수만 바꾼다
      if (phaseKind.current === "pick") void refresh();
      else void getPopupState().then((s) => setRemaining(s.remaining));
    });
  }, [refresh]);

  const next = useCallback(async () => {
    setPhase({ kind: "pick" });
    const state = await refresh();
    if (!state.current) await hidePopup();
  }, [refresh]);

  const put = useCallback(async () => {
    if (!current || !selected || busy) return;
    setBusy(true);
    setError(null);
    try {
      const result = await putFile(current.path, selected, newName);
      setPhase({ kind: "done", file: { ...current, name: newName ?? current.name }, result });
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }, [busy, current, newName, selected]);

  const later = useCallback(async () => {
    if (!current || busy) return;
    setBusy(true);
    try {
      await deferFile(current.path);
      await next();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }, [busy, current, next]);

  const undo = async () => {
    if (phase.kind !== "done" || busy) return;
    setBusy(true);
    try {
      await undoMove(phase.result.moveId, true);
      shownPath.current = null; // 돌아온 파일을 처음부터 다시 고르게
      setPhase({ kind: "pick" });
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const chooseOther = async () => {
    try {
      const picked = await pickFolder("넣을 폴더 고르기");
      if (picked) setSelected(picked);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const chooseRecent = (path: string) => {
    setSelected(path);
    setReveal(path);
  };

  const confirmRename = () => {
    if (editing === null || !current) return;
    const [, ext] = splitName(current.name);
    const stem = editing.trim();
    setNewName(stem && stem + ext !== current.name ? stem + ext : null);
    setEditing(null);
  };

  // 키보드: Enter = 넣기(정리 후에는 다음), Esc = 나중에 정리(정리 후에는 다음), 1·2·3 = 최근 넣은 곳
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement;
      if (phase.kind === "done") {
        if (e.key === "Enter" || e.key === "Escape") {
          e.preventDefault();
          void next();
        }
        return;
      }
      if (typing) return;
      if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) {
        e.preventDefault();
        void put();
      } else if (e.key === "Escape") {
        e.preventDefault();
        void later();
      } else if (["1", "2", "3"].includes(e.key)) {
        const r = recents[Number(e.key) - 1];
        if (r) chooseRecent(r);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [phase.kind, put, later, next, recents]);

  const header = (
    <header className="popup__head">
      <div className="popup__brand">
        <FolderIcon size={16} />
        <span>파일 정리 도우미</span>
        {remaining > 0 && <span className="popup__count">남은 파일 {remaining}개</span>}
      </div>
      <button
        type="button"
        className="popup__close"
        aria-label={phase.kind === "done" ? "닫기" : "나중에 정리하고 닫기"}
        onClick={() => void (phase.kind === "done" ? next() : later())}
      >
        <X size={16} />
      </button>
    </header>
  );

  if (phase.kind === "done") {
    return (
      <div className="popup">
        {header}
        <div className="popup__done">
          <div className="popup__done-row">
            <span className="popup__done-icon" aria-hidden="true">
              <Check size={20} />
            </span>
            <div className="popup__done-text">
              <div className="popup__done-title">정리했어요</div>
              <div className="popup__done-name">{baseName(phase.result.movedPath)}</div>
              <div className="popup__done-path">{displayPath(root, phase.result.destDir, sep)}</div>
            </div>
            <button type="button" className="btn btn--link" onClick={() => void undo()} disabled={busy}>
              되돌리기
            </button>
          </div>
          {error && <p className="error-text">{error}</p>}
        </div>
        <footer className="popup__foot">
          <div className="button-row popup__actions">
            <span className="popup__spacer" />
            <button type="button" className="btn btn--primary" onClick={() => void next()} autoFocus>
              {remaining > 0 ? `다음 파일 (${remaining})` : "닫기"}
            </button>
          </div>
        </footer>
      </div>
    );
  }

  if (!current || !root) {
    return (
      <div className="popup">
        {header}
        <div className="popup__empty">정리할 새 파일이 없어요.</div>
      </div>
    );
  }

  const shownName = newName ?? current.name;
  const [stem, ext] = splitName(shownName);

  return (
    <div className="popup">
      {header}

      <section className="popup__file">
        <FileBadge name={shownName} />
        <div className="popup__file-text">
          {editing === null ? (
            <div className="popup__file-name" title={shownName}>
              {shownName}
            </div>
          ) : (
            <div className="popup__rename button-row">
              <input
                autoFocus
                aria-label="새 파일 이름"
                value={editing}
                onChange={(e) => setEditing(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    confirmRename();
                  }
                  if (e.key === "Escape") {
                    e.stopPropagation();
                    setEditing(null);
                  }
                }}
                onBlur={confirmRename}
              />
              <span className="popup__ext">{ext}</span>
            </div>
          )}
          <div className="popup__file-meta">
            {friendlyName(current.sourceDir, info)}에 방금 저장됨 · {formatSize(current.size)}
            {newName && " · 이름 바뀜"}
          </div>
        </div>
        {editing === null && (
          <button
            type="button"
            className="icon-btn"
            aria-label="파일 이름 바꾸기"
            onClick={() => setEditing(stem)}
          >
            <Pencil size={15} />
          </button>
        )}
      </section>

      {recents.length > 0 && (
        <section className="popup__recent" aria-label="최근 넣은 곳">
          <div className="popup__label">최근 넣은 곳</div>
          <div className="popup__recent-list">
            {recents.map((r, i) => (
              <button
                key={r}
                type="button"
                className={`popup__recent-btn${r === selected ? " popup__recent-btn--on" : ""}`}
                title={displayPath(root, r, sep)}
                onClick={() => chooseRecent(r)}
              >
                <span className="popup__key">{i + 1}</span>
                <span className="popup__recent-name">{baseName(r)}</span>
              </button>
            ))}
          </div>
        </section>
      )}

      <section className="popup__folders" aria-label="폴더 고르기">
        <FolderPicker
          root={root}
          separator={sep}
          selected={selected}
          onSelect={setSelected}
          reveal={reveal}
        />
      </section>

      <footer className="popup__foot">
        <div className="popup__target">
          넣을 곳
          <strong className={selected ? "" : "popup__target--none"}>
            {selected ? displayPath(root, selected, sep) : "폴더를 골라 주세요"}
          </strong>
        </div>
        {error && <p className="error-text">{error}</p>}
        <div className="button-row popup__actions">
          <button type="button" className="btn btn--ghost popup__later" onClick={() => void later()} disabled={busy}>
            나중에 정리
          </button>
          <span className="popup__spacer" />
          <button type="button" className="btn popup__other" onClick={() => void chooseOther()} disabled={busy}>
            <FolderOpen size={16} aria-hidden="true" />
            다른 위치…
          </button>
          <button type="button" className="btn btn--primary" onClick={() => void put()} disabled={!selected || busy}>
            넣기
          </button>
        </div>
      </footer>
    </div>
  );
}
