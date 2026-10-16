import { useCallback, useEffect, useRef, useState } from "react";
import { Check, FolderOpen, Pencil, X } from "lucide-react";
import FileBadge from "../components/FileBadge";
import FolderIcon from "../components/FolderIcon";
import FolderPicker from "../components/FolderPicker";
import { RuleChangeCard, RuleDraftCard } from "../components/RuleCards";
import {
  deferFile,
  dismissAuto,
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
import { baseName, displayPath, friendlyName, joinPath } from "../lib/paths";
import { reasonLabel, ruleName } from "../lib/rules";
import type { AutoMoved, PendingFile, PutResult, Suggestion } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import "./Popup.css";

type Phase = { kind: "pick" } | { kind: "done"; file: PendingFile; result: PutResult };

/** "바로 정리" 알림은 이 시간 뒤 저절로 닫힌다(마우스를 올려 두면 기다림) */
const AUTO_NOTICE_MS = 8000;

/** 추천 폴더가 보이게 열 위치: 있는 폴더면 그 폴더, 아직 없으면(새 연도·학기) 지금 있는 가장 깊은 폴더 안 */
function revealFor(s: Suggestion, sep: string): string {
  return s.exists ? s.destDir : joinPath(s.reveal, baseName(s.destDir), sep);
}

// 새 파일 팝업(SPEC 3-1). 오른쪽 아래 작은 창. Enter = 넣기, Esc = 나중에 정리, 1·2·3 = 최근 넣은 곳.
export default function Popup() {
  const settings = useAppStore((s) => s.settings);
  const info = useAppStore((s) => s.setupInfo);
  const reloadSettings = useAppStore((s) => s.load);
  const root = settings?.rootFolder ?? null;
  const sep = info?.separator ?? "\\";

  const [current, setCurrent] = useState<PendingFile | null>(null);
  const [remaining, setRemaining] = useState(0);
  const [suggestion, setSuggestion] = useState<Suggestion | null>(null);
  const [autoMoved, setAutoMoved] = useState<AutoMoved[]>([]);
  const [hovering, setHovering] = useState(false);
  /** "응답 없으면 n초 뒤 추천 위치에 넣기": 남은 초(null = 세지 않음) */
  const [countdown, setCountdown] = useState<number | null>(null);
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
    setSuggestion(state.suggestion);
    setAutoMoved(state.autoMoved);
    // 다른 파일로 바뀌면 고른 것 초기화. 추천이 있으면 추천 폴더를 골라 둔 채 시작한다.
    if (state.current?.path !== shownPath.current) {
      shownPath.current = state.current?.path ?? null;
      setSelected(state.suggestion?.destDir ?? null);
      setReveal(state.suggestion ? revealFor(state.suggestion, sep) : null);
      setNewName(null);
      setEditing(null);
      setError(null);
      setRecents(await getRecentDestinations().catch(() => []));
    }
    return state;
  }, [reloadSettings, sep]);

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
    if (!state.current && state.autoMoved.length === 0) await hidePopup();
  }, [refresh]);

  // "바로 정리" 알림 닫기 → 물어볼 파일이 있으면 이어서, 없으면 창을 숨긴다
  const closeNotice = useCallback(async () => {
    await dismissAuto(autoMoved.map((a) => a.moveId)).catch(() => {});
    await next();
  }, [autoMoved, next]);

  const noticeShown = phase.kind === "pick" && autoMoved.length > 0;
  useEffect(() => {
    if (!noticeShown || hovering) return;
    const timer = window.setTimeout(() => void closeNotice(), AUTO_NOTICE_MS);
    return () => window.clearTimeout(timer);
  }, [noticeShown, hovering, closeNotice]);

  // 추천이 있는 새 파일이 보이면 세기 시작(설정 "추천만" + 응답 없으면 n초 뒤)
  const wait = settings?.placeMode === "suggest" ? (settings.waitSeconds ?? 0) : 0;
  const counting = phase.kind === "pick" && !noticeShown && !!current && !!suggestion && wait > 0;
  useEffect(() => {
    setCountdown(counting ? wait : null);
  }, [counting, wait, current?.path]);

  // 사용자가 무엇이든 누르면 멈춘다
  const isCounting = countdown !== null;
  useEffect(() => {
    if (!isCounting) return;
    const stop = () => setCountdown(null);
    window.addEventListener("pointerdown", stop, true);
    window.addEventListener("keydown", stop, true);
    return () => {
      window.removeEventListener("pointerdown", stop, true);
      window.removeEventListener("keydown", stop, true);
    };
  }, [isCounting]);

  const undoAuto = async (item: AutoMoved) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      // 되돌린 파일은 팝업에서 다시 물어본다
      await undoMove(item.moveId, true);
      shownPath.current = null;
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

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

  // 다 세면 추천 위치(골라 둔 폴더)에 넣는다
  useEffect(() => {
    if (countdown === null) return;
    if (countdown <= 0) {
      setCountdown(null);
      void put();
      return;
    }
    const timer = window.setTimeout(() => setCountdown((c) => (c === null ? null : c - 1)), 1000);
    return () => window.clearTimeout(timer);
  }, [countdown, put]);

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

  const chooseSuggestion = () => {
    if (!suggestion) return;
    setSelected(suggestion.destDir);
    setReveal(revealFor(suggestion, sep));
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
      if (phase.kind === "done" || noticeShown) {
        // 버튼·체크 상자 위의 Enter는 그 버튼이 받는다(예: ‘규칙으로 저장’)
        const onControl = typing || e.target instanceof HTMLButtonElement;
        if (e.key === "Escape" || (e.key === "Enter" && !onControl)) {
          e.preventDefault();
          void (noticeShown ? closeNotice() : next());
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
  }, [phase.kind, noticeShown, put, later, next, closeNotice, recents]);

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
        aria-label={phase.kind === "done" || noticeShown ? "닫기" : "나중에 정리하고 닫기"}
        onClick={() => void (noticeShown ? closeNotice() : phase.kind === "done" ? next() : later())}
      >
        <X size={16} />
      </button>
    </header>
  );

  if (phase.kind === "done") {
    const { result } = phase;
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
              <div className="popup__done-name">{baseName(result.movedPath)}</div>
              <div className="popup__done-path">{displayPath(root, result.destDir, sep)}</div>
              {result.followed && (
                <div className="popup__done-rule">
                  {result.followed.kind === "rule"
                    ? `${ruleName(result.followed.rule)}대로 넣었어요`
                    : `추천 위치에 넣었어요 · ${reasonLabel(result.followed)}`}
                </div>
              )}
            </div>
            <button type="button" className="btn btn--link" onClick={() => void undo()} disabled={busy}>
              되돌리기
            </button>
          </div>
          {error && <p className="error-text">{error}</p>}
          {root && result.change && <RuleChangeCard change={result.change} root={root} separator={sep} />}
          {root && result.draft && (
            <RuleDraftCard draft={result.draft} root={root} separator={sep} onSkip={() => void next()} />
          )}
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

  if (noticeShown) {
    const shown = autoMoved.slice(0, 4);
    return (
      <div
        className="popup"
        onMouseEnter={() => setHovering(true)}
        onMouseLeave={() => setHovering(false)}
      >
        {header}
        <div className="popup__done">
          <div className="popup__done-row">
            <span className="popup__done-icon" aria-hidden="true">
              <Check size={20} />
            </span>
            <div className="popup__done-text">
              <div className="popup__done-title">
                {autoMoved.length === 1 ? "자동으로 정리했어요" : `${autoMoved.length}개 파일을 자동으로 정리했어요`}
              </div>
              <div className="popup__done-path">잘못 넣었으면 되돌리기를 누르세요. 되돌린 파일은 다시 물어봐요.</div>
            </div>
          </div>
          <ul className="popup__auto">
            {shown.map((a) => (
              <li key={a.moveId} className="popup__auto-item">
                <FileBadge name={a.name} size={32} />
                <div className="popup__auto-text">
                  <div className="popup__done-name" title={a.name}>
                    {a.name}
                  </div>
                  <div className="popup__auto-meta" title={`${displayPath(root, a.destDir, sep)} · ${reasonLabel(a.reason)}`}>
                    {displayPath(root, a.destDir, sep)}
                  </div>
                </div>
                <button type="button" className="btn btn--link" onClick={() => void undoAuto(a)} disabled={busy}>
                  되돌리기
                </button>
              </li>
            ))}
          </ul>
          {autoMoved.length > shown.length && (
            <p className="popup__auto-more">그 밖에 {autoMoved.length - shown.length}개를 더 정리했어요</p>
          )}
          {error && <p className="error-text">{error}</p>}
        </div>
        <footer className="popup__foot">
          <div className="button-row popup__actions">
            <span className="popup__spacer" />
            <button type="button" className="btn btn--primary" onClick={() => void closeNotice()} autoFocus>
              확인
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

      {suggestion && (
        <section className="popup__suggest">
          <button
            type="button"
            className={`popup__suggest-btn${selected === suggestion.destDir ? " popup__suggest-btn--on" : ""}`}
            onClick={chooseSuggestion}
            aria-pressed={selected === suggestion.destDir}
          >
            <FolderIcon size={34} tone="blue" />
            <span className="popup__suggest-text">
              <span className="popup__suggest-label">추천 위치 · {reasonLabel(suggestion.reason)}</span>
              <span className="popup__suggest-path" title={displayPath(root, suggestion.destDir, sep)}>
                {displayPath(root, suggestion.destDir, sep)}
              </span>
            </span>
          </button>
        </section>
      )}

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
          recommended={suggestion?.destDir ?? null}
        />
      </section>

      <footer className="popup__foot">
        <div className="popup__target">
          넣을 곳
          <strong className={selected ? "" : "popup__target--none"}>
            {selected ? displayPath(root, selected, sep) : "폴더를 골라 주세요"}
          </strong>
          {suggestion && !suggestion.exists && selected === suggestion.destDir && (
            <span className="popup__target-new"> · 없는 폴더는 만들어요</span>
          )}
        </div>
        {countdown !== null && (
          <p className="popup__countdown" role="status">
            {countdown}초 뒤 추천 위치에 넣어요 · 아무 곳이나 누르면 멈춰요
          </p>
        )}
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
