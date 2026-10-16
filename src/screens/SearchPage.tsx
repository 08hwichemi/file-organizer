import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { FolderOpen, Search, X } from "lucide-react";
import FileBadge from "../components/FileBadge";
import {
  errorMessage,
  getSearchInfo,
  onBackendEvent,
  openPath,
  prepareSearch,
  revealPath,
  searchFiles,
} from "../lib/backend";
import { formatDate, formatSize } from "../lib/format";
import { displayPath } from "../lib/paths";
import type { SearchResponse, SearchSort } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import "./SearchPage.css";

const ALL = "";

// 다른 화면에 갔다 와도 검색어가 그대로 남게
let lastQuery = "";

const SORTS: { value: SearchSort; label: string }[] = [
  { value: "best", label: "이름에 맞는 것 먼저" },
  { value: "newest", label: "최근 수정한 것부터" },
  { value: "oldest", label: "오래된 것부터" },
  { value: "nameAsc", label: "이름 가나다순" },
  { value: "nameDesc", label: "이름 역순" },
  { value: "largest", label: "큰 파일부터" },
  { value: "smallest", label: "작은 파일부터" },
];
const SORT_KEY = "finder.sort";

/** 고른 정렬은 이 PC에 기억해 둔다(다음에 켜도 그대로) */
function savedSort(): SearchSort {
  try {
    const v = window.localStorage.getItem(SORT_KEY);
    return SORTS.some((s) => s.value === v) ? (v as SearchSort) : "best";
  } catch {
    return "best";
  }
}

/** 어디서 찾는지 한 줄로 */
function scopeText(r: SearchResponse | null, progress: number | null): string {
  if (!r) return "";
  const count = (n: number) => n.toLocaleString("ko-KR");
  if (r.source === "everything") return "PC 전체에서 찾아요 · Everything으로 찾는 중";
  if (r.source === "pc") {
    return r.indexing ? "PC 전체에서 찾아요 · 파일 목록을 새로 읽는 중" : `PC 전체에서 찾아요 · 파일 ${count(r.indexed)}개`;
  }
  if (r.wantWholePc && r.indexing) {
    return `PC 전체를 읽는 중이에요(파일 ${count(progress ?? r.indexed)}개). 다 읽을 때까지는 정리 폴더와 지켜볼 폴더에서 찾아요.`;
  }
  return "정리 폴더와 지켜볼 폴더에서 찾아요";
}

/**
 * 파일 검색(SPEC 3-3·6장): 메인 창 오른쪽 화면. 왼쪽 메뉴 "파일 검색", 단축키(기본 Alt+Space), 트레이로 연다.
 * 입력하면 바로 찾고, ↑↓로 고르고, Enter = 열기, Ctrl+Enter = 위치 열기, Esc = 검색어 지우기.
 * 연도·학기·학급으로 거를 수 있다(파일 이름 → 없으면 폴더 이름에서 읽은 값).
 */
export default function SearchPage() {
  const settings = useAppStore((s) => s.settings);
  const sep = useAppStore((s) => s.setupInfo?.separator ?? "\\");
  const focusToken = useAppStore((s) => s.searchFocus);
  const goTo = useAppStore((s) => s.goTo);
  const root = settings?.rootFolder ?? null;

  const [query, setQuery] = useState(lastQuery);
  const [response, setResponse] = useState<SearchResponse | null>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const [active, setActive] = useState(0);
  const [year, setYear] = useState(ALL);
  const [term, setTerm] = useState(ALL);
  const [klass, setKlass] = useState(ALL);
  const [sort, setSort] = useState<SearchSort>(savedSort);
  const [error, setError] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const requestId = useRef(0);

  // 정렬이 바뀌면 다시 찾는다(최대 개수로 자르기 전에 정렬해야 맞으므로 받은 결과만 섞지 않는다)
  const sortRef = useRef(sort);
  sortRef.current = sort;
  const run = useCallback((q: string) => {
    const id = ++requestId.current;
    searchFiles(q, sortRef.current)
      .then((r) => {
        if (id !== requestId.current) return; // 늦게 온 예전 결과는 버린다
        setResponse(r);
        setActive(0);
        setError(null);
      })
      .catch((e) => id === requestId.current && setError(errorMessage(e)));
  }, []);

  // 화면을 열 때: 파일 목록이 없거나 오래됐으면 뒤에서 읽기 시작
  useEffect(() => {
    void prepareSearch().catch(() => {});
  }, []);

  // 입력이 멈추면(또는 정렬을 바꾸면) 찾는다
  useEffect(() => {
    lastQuery = query;
    const timer = window.setTimeout(() => run(query), 120);
    return () => window.clearTimeout(timer);
  }, [query, sort, run]);

  const changeSort = (v: SearchSort) => {
    setSort(v);
    try {
      window.localStorage.setItem(SORT_KEY, v);
    } catch {
      // 기억하지 못해도 이번에는 그대로 쓴다
    }
  };

  // 단축키·트레이로 열 때마다(그리고 처음에): 입력 칸에 초점, 글자 전체 선택
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, [focusToken]);

  // 파일 목록을 다 읽으면 지금 검색어로 다시 찾는다
  useEffect(() => onBackendEvent("search-indexed", () => run(input.current?.value ?? "")), [run]);

  // 읽는 동안에는 읽은 파일 수를 자주 새로 보여 준다
  const indexing = response?.indexing ?? false;
  useEffect(() => {
    if (!indexing) {
      setProgress(null);
      return;
    }
    const timer = window.setInterval(() => {
      void getSearchInfo()
        .then((info) => {
          setProgress(info.indexed);
          if (!info.indexing) run(input.current?.value ?? "");
        })
        .catch(() => {});
    }, 1000);
    return () => window.clearInterval(timer);
  }, [indexing, run]);

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
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const clear = () => {
    setQuery("");
    input.current?.focus();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      clear();
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

  const hasFilters = options.years.length + options.terms.length + options.classes.length > 0;
  const filtered = year !== ALL || term !== ALL || klass !== ALL;
  const onlyFolders = response?.source === "folders" && !response.wantWholePc;

  return (
    <div className="page page--fill">
      <div>
        <h1 className="page__title">파일 검색</h1>
        <p className="page__lead">
          파일 이름의 일부만 적어도 찾아요. 띄어 쓰면 낱말이 모두 들어 있는 파일을 찾아요(폴더 이름도 함께 봐요).
        </p>
      </div>

      <div className="finder" onKeyDown={onKeyDown}>
        <div className="finder__bar">
          <Search size={20} aria-hidden="true" className="finder__icon" />
          <input
            ref={input}
            autoFocus
            className="finder__input"
            aria-label="찾을 파일 이름"
            placeholder="찾을 파일 이름(예: 수행평가 2027)"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {query && (
            <button type="button" className="finder__clear" aria-label="검색어 지우기" title="지우기(Esc)" onClick={clear}>
              <X size={18} />
            </button>
          )}
        </div>

        <div className="finder__sub">
          <span className="finder__scope">
            {scopeText(response, progress)}
            {onlyFolders && (
              <button type="button" className="finder__link" onClick={() => goTo("settings")}>
                범위 바꾸기
              </button>
            )}
          </span>
          <div className="finder__filters">
            <select
              className={`finder__select${sort !== "best" ? " finder__select--on" : ""}`}
              aria-label="정렬"
              title="정렬"
              value={sort}
              onChange={(e) => changeSort(e.target.value as SearchSort)}
            >
              {SORTS.map((s) => (
                <option key={s.value} value={s.value}>
                  {s.label}
                </option>
              ))}
            </select>
            {hasFilters && (
              <>
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
              </>
            )}
          </div>
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
                    {` · ${formatSize(h.size)}`}
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
              : filtered
                ? "고른 연도·학기·학급에 맞는 파일이 없어요."
                : response?.indexing
                  ? "아직 찾지 못했어요. 파일 목록을 다 읽으면 다시 찾아 드릴게요."
                  : "찾는 파일이 없어요."}
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
            <kbd>Esc</kbd> 지우기
          </span>
          {response && response.hits.length > 0 && (
            <span className="finder__count">
              {filtered ? `${hits.length} / ${response.hits.length}개` : `${response.hits.length}개`}
              {response.hits.length >= 200 && " (더 자세히 적으면 줄어요)"}
            </span>
          )}
        </footer>
      </div>
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
