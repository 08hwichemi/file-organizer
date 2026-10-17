import { useEffect, useRef, useState } from "react";
import { ChevronRight, FolderPlus, Search, X } from "lucide-react";
import { createFolder, errorMessage, listFolders, onBackendEvent, searchFolders } from "../lib/backend";
import { baseName, parentPath, samePath, trailFor, type Crumb } from "../lib/paths";
import type { FolderEntry, FolderMatch } from "../lib/types";
import FolderIcon from "./FolderIcon";
import PathBar from "./PathBar";
import "./FolderPicker.css";

interface Props {
  root: string;
  separator: string;
  /** 지금 고른 폴더(넣을 곳) */
  selected: string | null;
  onSelect: (path: string) => void;
  /** 이 값이 바뀌면 그 폴더가 보이도록 상위 폴더를 연다(최근 넣은 곳·검색 결과 고를 때). */
  reveal: string | null;
  /** 추천 위치(그 폴더 칸에 "추천" 표시) */
  recommended?: string | null;
  /** 아래 "한 번 누르면 선택…" 안내 줄(위 띠에 같은 안내가 있으면 끈다) */
  hint?: boolean;
}

/**
 * 팝업의 폴더 창(SPEC 3-1 ④). 탐색기처럼 폴더 아이콘 격자, 한 줄 4개.
 * 한 번 누르면 선택, 두 번 누르거나 › 를 누르면 안으로. 돋보기 = 폴더 이름 검색. 마지막 칸 = 새 폴더.
 */
export default function FolderPicker({ root, separator, selected, onSelect, reveal, recommended, hint = true }: Props) {
  const [trail, setTrail] = useState<Crumb[]>([{ name: baseName(root), path: root }]);
  const [folders, setFolders] = useState<FolderEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [searching, setSearching] = useState(false);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<FolderMatch[] | null>(null);
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [reloadKey, setReloadKey] = useState(0);
  const searchInput = useRef<HTMLInputElement>(null);

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

  // 다른 곳(정리함의 폴더 칸, 탐색기 등)에서 폴더가 생기거나 바뀌면 바로 새로 읽는다
  useEffect(() => {
    const reload = () => setReloadKey((k) => k + 1);
    const stops = [onBackendEvent("folders-changed", reload), onBackendEvent("inbox-changed", reload)];
    return () => stops.forEach((stop) => stop());
  }, []);

  // 최근 넣은 곳·검색 결과를 고르면 그 폴더가 보이는 위치로
  useEffect(() => {
    if (!reveal) return;
    const t = trailFor(root, parentPath(reveal, separator), separator);
    if (t) setTrail(t);
  }, [reveal, root, separator]);

  // 검색(입력 후 잠깐 기다렸다가)
  useEffect(() => {
    if (!searching || !query.trim()) {
      setResults(null);
      return;
    }
    let alive = true;
    const timer = window.setTimeout(() => {
      searchFolders(query)
        .then((r) => alive && setResults(r))
        .catch(() => alive && setResults([]));
    }, 150);
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, [query, searching]);

  const enter = (f: FolderEntry) => setTrail((t) => [...t, { name: f.name, path: f.path }]);

  const toggleSearch = () => {
    setSearching((s) => !s);
    setQuery("");
    setCreating(false);
    setTimeout(() => searchInput.current?.focus(), 0);
  };

  const make = async () => {
    try {
      const made = await createFolder(current.path, newName);
      setCreating(false);
      setNewName("");
      setReloadKey((k) => k + 1);
      onSelect(made.path);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <div className="picker">
      <PathBar
        compact
        trail={trail}
        onGo={(i) => {
          setSearching(false);
          setTrail((t) => t.slice(0, i + 1));
        }}
      >
        <button
          type="button"
          className={`icon-btn${searching ? " icon-btn--on" : ""}`}
          aria-label={searching ? "찾기 닫기" : "폴더 찾기"}
          aria-pressed={searching}
          onClick={toggleSearch}
        >
          {searching ? <X size={17} /> : <Search size={17} />}
        </button>
      </PathBar>

      {searching && (
        <input
          ref={searchInput}
          className="picker__input"
          aria-label="폴더 이름으로 찾기"
          placeholder="폴더 이름으로 찾기"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && (e.stopPropagation(), toggleSearch())}
        />
      )}

      {creating && (
        <div className="picker__new button-row">
          <input
            autoFocus
            className="picker__input"
            aria-label="새 폴더 이름"
            placeholder={`${current.name} 안에 만들 폴더 이름`}
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                e.stopPropagation();
                void make();
              }
              if (e.key === "Escape") {
                e.stopPropagation();
                setCreating(false);
              }
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

      {error && <p className="error-text">{error}</p>}

      <div className="picker__scroll">
        {searching && query.trim() ? (
          <ul className="picker__results">
            {results?.map((r) => (
              <li key={r.path}>
                <button
                  type="button"
                  className={`picker__result${r.path === selected ? " picker__result--on" : ""}`}
                  onClick={() => {
                    onSelect(r.path);
                    setSearching(false);
                    const t = trailFor(root, parentPath(r.path, separator), separator);
                    if (t) setTrail(t);
                  }}
                >
                  <FolderIcon size={20} />
                  <span>{r.trail.join(" › ")}</span>
                </button>
              </li>
            ))}
            {results && results.length === 0 && <li className="picker__empty">찾는 폴더가 없어요</li>}
          </ul>
        ) : (
          <>
            <ul className="picker__grid">
              {folders?.map((f) => {
                const on = f.path === selected;
                return (
                  <li key={f.path} className={`picker__tile${on ? " picker__tile--on" : ""}`}>
                    <button
                      type="button"
                      className="picker__pick"
                      aria-label={`${f.name} 선택`}
                      aria-pressed={on}
                      onClick={() => onSelect(f.path)}
                      onDoubleClick={() => enter(f)}
                    >
                      <FolderIcon size={44} tone={on ? "blue" : "yellow"} />
                      <span className="picker__name" title={f.name}>
                        {f.name}
                      </span>
                    </button>
                    {recommended && samePath(recommended, f.path) && (
                      <span className="picker__badge" aria-label="추천">
                        추천
                      </span>
                    )}
                    {f.folderCount > 0 && (
                      <button
                        type="button"
                        className="picker__enter"
                        aria-label={`${f.name} 열기`}
                        onClick={() => enter(f)}
                      >
                        <ChevronRight size={16} />
                      </button>
                    )}
                  </li>
                );
              })}
              <li>
                <button
                  type="button"
                  className="picker__add"
                  onClick={() => {
                    setCreating(true);
                    setSearching(false);
                  }}
                >
                  <FolderPlus size={20} aria-hidden="true" />
                  <span>새 폴더</span>
                </button>
              </li>
            </ul>
            {hint && <p className="picker__hint">한 번 누르면 선택, 두 번 누르거나 › 를 누르면 안으로 들어가요</p>}
          </>
        )}
      </div>
    </div>
  );
}
