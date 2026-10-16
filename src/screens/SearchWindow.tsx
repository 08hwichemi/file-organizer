import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { FolderOpen, Search, X } from "lucide-react";
import FileBadge from "../components/FileBadge";
import {
  errorMessage,
  hideSearch,
  onBackendEvent,
  openPath,
  revealPath,
  searchFiles,
} from "../lib/backend";
import { formatDate } from "../lib/format";
import { displayPath } from "../lib/paths";
import type { SearchResponse } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import "./SearchWindow.css";

const ALL = "";

/**
 * 파일 검색창(SPEC 3-3·6장). 단축키(기본 Alt+Space)·트레이·메인 창에서 연다.
 * 입력하면 바로 찾고, ↑↓로 고르고, Enter = 열기, Ctrl+Enter = 위치 열기, Esc = 닫기.
 * 연도·학기·학급으로 거를 수 있다(파일 이름 → 없으면 폴더 이름에서 읽은 값).
 */
export default function SearchWindow() {
  const settings = useAppStore((s) => s.settings);
  const reloadSettings = useAppStore((s) => s.load);
  const sep = useAppStore((s) => s.setupInfo?.separator ?? "\\");
  const root = settings?.rootFolder ?? null;

  const [query, setQuery] = useState("");
  const [response, setResponse] = useState<SearchResponse | null>(null);
  const [active, setActive] = useState(0);
  const [year, setYear] = useState(ALL);
  const [term, setTerm] = useState(ALL);
  const [klass, setKlass] = useState(ALL);
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const requestId = useRef(0);

  const run = useCallback((q: string) => {
    const id = ++requestId.current;
    searchFiles(q)
      .then((r) => {
        if (id !== requestId.current) return; // 늦게 온 예전 결과는 버린다
        setResponse(r);
        setActive(0);
        setError(null);
      })
      .catch((e) => id === requestId.current && setError(errorMessage(e)));
  }, []);

  // 입력이 멈추면 찾는다
  useEffect(() => {
    const timer = window.setTimeout(() => run(query), 120);
    return () => window.clearTimeout(timer);
  }, [query, run]);

  // 열릴 때마다: 입력 칸에 초점, 글자 전체 선택, 설정·결과 새로
  useEffect(
    () =>
      onBackendEvent("search-opened", () => {
        void reloadSettings();
        input.current?.focus();
        input.current?.select();
        run(input.current?.value ?? "");
      }),
    [reloadSettings, run],
  );

  const hits = useMemo(() => {
    const all = response?.hits ?? [];
    return all.filter(
      (h) =>
        (year === ALL || String(h.year) === year) &&
        (term === ALL || String(h.term) === term) &&
        (klass === ALL || h.class === klass),
    );
  }, [response, year, term, klass]);

  // 거르기 칸에 보여 줄 값(지금 결과에 있는 것만)
  const options = useMemo(() => {
    const all = response?.hits ?? [];
    const uniq = <T,>(xs: (T | null)[]) => [...new Set(xs.filter((x): x is T => x !== null))];
    return {
      years: uniq(all.map((h) => h.year)).sort((a, b) => b - a),
      terms: uniq(all.map((h) => h.term)).sort(),
      classes: uniq(all.map((h) => h.class)).sort((a, b) => a.localeCompare(b, "ko", { numeric: true })),
    };
  }, [response]);

  // 새 결과에 없는 값으로 걸러 두었으면 풀어 준다(예전 검색의 학급이 남아 아무것도 안 보이지 않게)
  useEffect(() => {
    if (year !== ALL && !options.years.some((y) => String(y) === year)) setYear(ALL);
    if (term !== ALL && !options.terms.some((t) => String(t) === term)) setTerm(ALL);
    if (klass !== ALL && !options.classes.includes(klass)) setKlass(ALL);
  }, [options, year, term, klass]);

  // 고른 칸이 보이게
  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const act = async (kind: "open" | "reveal", path: string) => {
    try {
      await (kind === "open" ? openPath(path) : revealPath(path));
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      void hideSearch();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(a + 1, hits.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter" && hits[active]) {
      e.preventDefault();
      void act(e.ctrlKey ? "reveal" : "open", hits[active].path);
    }
  };

  const scope =
    response?.source === "everything"
      ? "PC 전체에서 찾아요(Everything)"
      : "정리 폴더와 지켜볼 폴더에서 찾아요";
  const hasFilters = options.years.length + options.terms.length + options.classes.length > 0;
  const filtered = year !== ALL || term !== ALL || klass !== ALL;

  return (
    <div className="finder" onKeyDown={onKeyDown}>
      <div className="finder__bar">
        <Search size={20} aria-hidden="true" className="finder__icon" />
        <input
          ref={input}
          autoFocus
          className="finder__input"
          aria-label="찾을 파일 이름"
          placeholder="찾을 파일 이름(띄어 쓰면 모두 들어 있는 파일)"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button type="button" className="finder__close" aria-label="검색창 닫기" onClick={() => void hideSearch()}>
          <X size={18} />
        </button>
      </div>

      <div className="finder__sub">
        <span className="finder__scope">{scope}</span>
        {hasFilters && (
          <div className="finder__filters">
            <FilterSelect label="연도" value={year} onChange={setYear}>
              {options.years.map((y) => (
                <option key={y} value={String(y)}>
                  {y}
                </option>
              ))}
            </FilterSelect>
            <FilterSelect label="학기" value={term} onChange={setTerm}>
              {options.terms.map((t) => (
                <option key={t} value={String(t)}>
                  {t}학기
                </option>
              ))}
            </FilterSelect>
            <FilterSelect label="학급" value={klass} onChange={setKlass}>
              {options.classes.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </FilterSelect>
          </div>
        )}
      </div>

      {error && <p className="finder__error error-text">{error}</p>}

      {hits.length > 0 && (
      <ul className="finder__list" ref={listRef} role="listbox" aria-label="찾은 파일">
        {hits.map((h, i) => (
          <li
            key={h.path}
            data-index={i}
            role="option"
            aria-selected={i === active}
            className={`finder__row${i === active ? " finder__row--on" : ""}`}
            onMouseMove={() => setActive(i)}
            onDoubleClick={() => void act("open", h.path)}
          >
            <FileBadge name={h.name} size={34} />
            <div className="finder__text">
              <div className="finder__name" title={h.name}>
                {h.name}
              </div>
              <div className="finder__meta" title={h.dir}>
                {displayPath(root, h.dir, sep)}
                {h.modified > 0 && ` · ${formatDate(h.modified)}`}
                {h.rule && <span className="finder__rule">‘{h.rule}’ 규칙</span>}
              </div>
            </div>
            <div className="button-row finder__actions">
              <button type="button" className="btn" onClick={() => void act("reveal", h.path)} title="위치 열기(Ctrl+Enter)">
                <FolderOpen size={15} aria-hidden="true" />
                위치
              </button>
              <button type="button" className="btn btn--accent" onClick={() => void act("open", h.path)} title="열기(Enter)">
                열기
              </button>
            </div>
          </li>
        ))}
      </ul>
      )}

      {hits.length === 0 && (
        <div className="finder__empty">
          {!query.trim()
            ? "찾을 파일 이름을 입력하세요."
            : response?.indexing && response.source === "index"
              ? "파일 목록을 읽는 중이에요. 잠시 뒤 다시 찾아 주세요."
              : filtered
                ? "고른 연도·학기·학급에 맞는 파일이 없어요."
                : "찾는 파일이 없어요."}
          {response?.source === "index" && response.everything === "notRunning" && (
            <div className="finder__hint">Everything을 설치해 켜 두면 PC 전체에서 찾을 수 있어요.</div>
          )}
        </div>
      )}

      <footer className="finder__foot">
        <span>
          <kbd>↑</kbd>
          <kbd>↓</kbd> 고르기
        </span>
        <span>
          <kbd>Enter</kbd> 열기
        </span>
        <span>
          <kbd>Ctrl</kbd>+<kbd>Enter</kbd> 위치 열기
        </span>
        <span>
          <kbd>Esc</kbd> 닫기
        </span>
        {response && response.hits.length > 0 && (
          <span className="finder__count">
            {filtered ? `${hits.length} / ${response.hits.length}개` : `${response.hits.length}개`}
          </span>
        )}
      </footer>
    </div>
  );
}

function FilterSelect({
  label,
  value,
  onChange,
  children,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  children: React.ReactNode;
}) {
  return (
    <select
      className={`finder__select${value !== ALL ? " finder__select--on" : ""}`}
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      <option value={ALL}>{label} 전체</option>
      {children}
    </select>
  );
}
