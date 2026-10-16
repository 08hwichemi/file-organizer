import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowRight, FolderOpen, GripVertical, ListOrdered, Plus, X } from "lucide-react";
import FileBadge from "../components/FileBadge";
import { errorMessage, pickFolder, renameApply, renameList, renamePreview, renameUndo } from "../lib/backend";
import { formatDate } from "../lib/format";
import type { Block, FirstFile, RenameFile, RenamePreview, RenameRequest, RenameStatus } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import "./RenamePage.css";

type Order = "name" | "date" | "manual";

interface Toast {
  message: string;
  batch: number | null;
  errors: string[];
}

const STATUS_TEXT: Record<RenameStatus, string> = {
  same: "그대로",
  rename: "바꿈",
  conflict: "번호 붙임",
  invalid: "건너뜀",
  skip: "건너뜀",
};

const SAMPLE_ROSTER = "번호\t이름\n1\t김민지\n2\t이서준\n3\t박하은";

const byName = (a: RenameFile, b: RenameFile) =>
  a.subDir.localeCompare(b.subDir, "ko", { numeric: true }) || a.name.localeCompare(b.name, "ko", { numeric: true });

const stemOf = (name: string) => (name.lastIndexOf(".") > 0 ? name.slice(0, name.lastIndexOf(".")) : name);

/**
 * 이름 한꺼번에 바꾸기(SPEC 3-4).
 * ① 폴더와 파일 → ② 첫 파일을 원하는 이름으로 적기(엑셀 빠른 채우기처럼) → 알아들은 블록 확인·고치기 → ③ 미리보기 → 실행.
 * 명렬표는 이 화면의 상태로만 들고 있다가 화면을 떠나면 사라진다(저장하지 않음).
 */
export default function RenamePage() {
  const userKind = useAppStore((s) => s.settings?.userKind ?? "school");
  const listWord = userKind === "general" ? "번호·이름 목록" : "명렬표";

  const [folder, setFolder] = useState<string | null>(null);
  const [includeSub, setIncludeSub] = useState(false);
  const [files, setFiles] = useState<RenameFile[]>([]);
  const [truncated, setTruncated] = useState(false);
  const [unchecked, setUnchecked] = useState<Set<string>>(new Set());
  const [order, setOrder] = useState<Order>("name");
  const [manual, setManual] = useState<string[]>([]);
  const [dragging, setDragging] = useState<string | null>(null);

  /** 첫 파일에 적은 새 이름(확장자 빼고) */
  const [example, setExample] = useState("");
  /** 블록을 직접 고쳤으면 그 블록(null이면 example로 알아듣기) */
  const [override, setOverride] = useState<Block[] | null>(null);
  const [roster, setRoster] = useState("");
  const [rosterOpen, setRosterOpen] = useState(false);

  const [preview, setPreview] = useState<RenamePreview | null>(null);
  const [menu, setMenu] = useState<number | "add" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<Toast | null>(null);
  const exampleInput = useRef<HTMLInputElement>(null);
  const requestId = useRef(0);

  const load = async (path: string, sub: boolean) => {
    try {
      const list = await renameList(path, sub);
      setFolder(list.folder);
      setFiles(list.files);
      setTruncated(list.truncated);
      setManual([...list.files].sort(byName).map((f) => f.path));
      setUnchecked(new Set());
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const choose = async () => {
    const picked = await pickFolder("이름을 바꿀 파일이 있는 폴더 고르기").catch(() => null);
    if (picked) {
      setToast(null);
      setOverride(null);
      setRosterOpen(false);
      await load(picked, includeSub);
    }
  };

  // 보이는 순서
  const ordered = useMemo(() => {
    if (order === "manual") {
      const map = new Map(files.map((f) => [f.path, f]));
      const known = manual.map((p) => map.get(p)).filter((f): f is RenameFile => !!f);
      const rest = files.filter((f) => !manual.includes(f.path)).sort(byName);
      return [...known, ...rest];
    }
    if (order === "date") return [...files].sort((a, b) => a.modified - b.modified || byName(a, b));
    return [...files].sort(byName);
  }, [files, order, manual]);

  const checked = useMemo(() => ordered.filter((f) => !unchecked.has(f.path)), [ordered, unchecked]);
  const firstPath = checked[0]?.path ?? null;

  // 첫 파일이 바뀌면(폴더·순서·체크) 적어 둔 이름을 그 파일 이름으로 되돌린다. 직접 고친 블록은 그대로 쓴다.
  useEffect(() => {
    if (override === null) setExample(checked[0] ? stemOf(checked[0].name) : "");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [firstPath]);

  const request: RenameRequest = useMemo(
    () => ({ files: checked.map((f) => f.path), example, blocks: override, roster, listWord }),
    [checked, example, override, roster, listWord],
  );

  // 바꿀 때마다 미리보기(잠깐 멈추면)
  useEffect(() => {
    if (request.files.length === 0) {
      setPreview(null);
      return;
    }
    const id = ++requestId.current;
    const timer = window.setTimeout(() => {
      renamePreview(request)
        .then((p) => {
          if (id !== requestId.current) return;
          setPreview(p);
          if (p.openRoster) setRosterOpen(true);
        })
        .catch((e) => id === requestId.current && setError(errorMessage(e)));
    }, 150);
    return () => window.clearTimeout(timer);
  }, [request]);

  // 블록을 직접 고쳤으면 첫 칸에도 그 결과를 보여 준다
  useEffect(() => {
    if (override === null || !preview?.rows[0] || !preview.first) return;
    const row = preview.rows[0];
    if (row.status === "rename" || row.status === "conflict") setExample(stemOf(row.newName));
    else if (row.status === "same") setExample(preview.first.stem);
  }, [preview, override]);

  // 블록 메뉴: 바깥을 누르거나 Esc면 닫기
  useEffect(() => {
    if (menu === null) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    document.addEventListener("click", close);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("click", close);
      document.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  const typeExample = (text: string) => {
    setOverride(null);
    setExample(text);
  };

  const blocks = preview?.blocks ?? [];
  const first = preview?.first ?? null;
  const editBlocks = (next: Block[]) => {
    setMenu(null);
    setOverride(next);
  };

  const quick = (kind: "year" | "today" | "seq" | "student") => {
    if (!first) return;
    if (kind === "year") typeExample(`${first.year}_${first.stem}`);
    else if (kind === "today") typeExample(`${first.stem}_${first.today}`);
    else if (kind === "seq") {
      typeExample("새 이름 1");
      window.requestAnimationFrame(() => {
        exampleInput.current?.focus();
        exampleInput.current?.setSelectionRange(0, 4);
      });
    } else {
      setRosterOpen(true);
      editBlocks([{ k: "student" }, { k: "text", text: "(" }, { k: "num", plain: true }, { k: "text", text: ")" }]);
    }
  };

  const toggle = (path: string) =>
    setUnchecked((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const dropOn = (target: string) => {
    if (!dragging || dragging === target) return;
    const list = ordered.map((f) => f.path).filter((p) => p !== dragging);
    list.splice(list.indexOf(target), 0, dragging);
    setManual(list);
    setDragging(null);
  };

  const apply = async () => {
    if (!preview || preview.changes === 0 || !folder) return;
    setBusy(true);
    try {
      // 화면에 보이는 블록 그대로 바꾼다
      const result = await renameApply({ ...request, blocks: preview.blocks });
      setToast({
        message: result.renamed > 0 ? `${result.renamed}개 파일 이름을 바꿨어요` : "이름을 바꾼 파일이 없어요",
        batch: result.renamed > 0 ? result.batch : null,
        errors: result.errors,
      });
      setOverride(null);
      await load(folder, includeSub);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const undo = async () => {
    if (!toast?.batch || !folder) return;
    setBusy(true);
    try {
      const result = await renameUndo(toast.batch);
      setToast({ message: `${result.restored}개 파일 이름을 되돌렸어요`, batch: null, errors: result.errors });
      setOverride(null);
      await load(folder, includeSub);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const rows = preview?.rows ?? [];
  const pathToFile = useMemo(() => new Map(files.map((f) => [f.path, f])), [files]);
  const counts = useMemo(() => {
    const c = { change: 0, same: 0, skip: 0 };
    for (const r of rows) {
      if (r.status === "rename" || r.status === "conflict") c.change += 1;
      else if (r.status === "same") c.same += 1;
      else c.skip += 1;
    }
    return c;
  }, [rows]);
  const showRoster = rosterOpen || (preview?.openRoster ?? false);
  const rosterCount = preview?.roster.count ?? 0;

  return (
    <div className="page rename">
      <div>
        <h1 className="page__title">이름 바꾸기</h1>
        <p className="page__lead">
          첫 파일을 원하는 이름으로 고쳐 적으면, 나머지 파일도 같은 모양으로 바꿔 드려요. 확장자는 그대로 두고, 같은
          이름이 있으면 덮어쓰지 않고 (2)를 붙여요. 바꾼 뒤에도 한 번에 되돌릴 수 있어요.
        </p>
      </div>

      {/* ① 폴더와 파일 */}
      <section className="rename__card" aria-labelledby="rename-step1">
        <h2 className="rename__step" id="rename-step1">
          <span className="rename__num">1</span>폴더와 파일
        </h2>
        <div className="rename__folder">
          <button type="button" className="btn" onClick={() => void choose()}>
            <FolderOpen size={16} aria-hidden="true" />
            폴더 고르기
          </button>
          <span className={`rename__path${folder ? "" : " rename__path--empty"}`} title={folder ?? undefined}>
            {folder ?? "아직 고른 폴더가 없어요"}
          </span>
          <label className="rename__check">
            <input
              type="checkbox"
              checked={includeSub}
              onChange={(e) => {
                setIncludeSub(e.target.checked);
                if (folder) void load(folder, e.target.checked);
              }}
            />
            하위 폴더 파일도
          </label>
        </div>
        {error && (
          <p className="error-text" role="alert">
            {error}
          </p>
        )}

        {folder && (
          <>
            <div className="rename__listbar">
              <span className="rename__count">
                {files.length}개 중 <strong>{checked.length}개</strong> 고름
                {truncated && " (너무 많아 앞쪽 3,000개만)"}
              </span>
              <div className="button-row rename__listtools">
                <button
                  type="button"
                  className="btn btn--small"
                  onClick={() => setUnchecked(checked.length === files.length ? new Set(files.map((f) => f.path)) : new Set())}
                >
                  {checked.length === files.length ? "모두 빼기" : "모두 고르기"}
                </button>
                <select className="rename__select" aria-label="순서" value={order} onChange={(e) => setOrder(e.target.value as Order)}>
                  <option value="name">이름순</option>
                  <option value="date">고친 날짜순</option>
                  <option value="manual">직접(끌어서)</option>
                </select>
              </div>
            </div>
            {files.length === 0 ? (
              <p className="rename__empty">이 폴더에는 바꿀 파일이 없어요.</p>
            ) : (
              <ul className="rename__files" aria-label="파일">
                {ordered.map((f) => (
                  <li
                    key={f.path}
                    className={`rename__file${dragging === f.path ? " rename__file--drag" : ""}`}
                    draggable={order === "manual"}
                    onDragStart={() => setDragging(f.path)}
                    onDragOver={(e) => order === "manual" && e.preventDefault()}
                    onDrop={() => dropOn(f.path)}
                    onDragEnd={() => setDragging(null)}
                  >
                    {order === "manual" && <GripVertical size={15} className="rename__grip" aria-hidden="true" />}
                    <input type="checkbox" aria-label={`${f.name} 고르기`} checked={!unchecked.has(f.path)} onChange={() => toggle(f.path)} />
                    <FileBadge name={f.name} size={26} />
                    <span className="rename__fname" title={f.name}>
                      {f.name}
                    </span>
                    {f.subDir && <span className="rename__sub">{f.subDir}</span>}
                    <span className="rename__date">{formatDate(f.modified)}</span>
                  </li>
                ))}
              </ul>
            )}
          </>
        )}
      </section>

      {/* ② 첫 파일을 원하는 이름으로 */}
      {folder && checked.length > 0 && first && (
        <section className="rename__card" aria-labelledby="rename-step2">
          <h2 className="rename__step" id="rename-step2">
            <span className="rename__num">2</span>첫 파일을 원하는 이름으로 적어 보세요
          </h2>
          <div className="rename__first">
            <span className="rename__old">{first.name}</span>
            <ArrowRight size={18} className="rename__arrow" aria-hidden="true" />
            <label className="rename__newbox">
              <input
                ref={exampleInput}
                aria-label="첫 파일의 새 이름"
                spellCheck={false}
                value={example}
                onChange={(e) => typeExample(e.target.value)}
              />
              <span className="rename__ext">{first.ext}</span>
            </label>
          </div>

          <div className="rename__quick">
            <span className="rename__muted">자주 쓰는 것</span>
            <button type="button" className="rename__qbtn" onClick={() => quick("year")}>
              앞에 올해 붙이기
            </button>
            <button type="button" className="rename__qbtn" onClick={() => quick("today")}>
              뒤에 오늘 날짜 붙이기
            </button>
            <button type="button" className="rename__qbtn" onClick={() => quick("seq")}>
              새 이름 + 번호
            </button>
            <button type="button" className="rename__qbtn" onClick={() => quick("student")}>
              번호를 이름으로
            </button>
            <button
              type="button"
              className={`rename__qbtn rename__roster-btn${showRoster ? " rename__roster-btn--on" : ""}`}
              aria-expanded={showRoster}
              onClick={() => setRosterOpen(!showRoster)}
            >
              <ListOrdered size={15} aria-hidden="true" />
              {listWord}
              {rosterCount > 0 && <span className="rename__badge">{rosterCount}명</span>}
            </button>
          </div>

          <div className="rename__understood">
            <div className="rename__u-title">
              이렇게 알아들었어요 <span className="rename__muted">· 블록을 누르면 바꾸거나 뺄 수 있어요</span>
            </div>
            <div className="rename__blocks">
              {blocks.length === 0 && <span className="rename__muted">위 칸에 새 이름을 적으면 여기에 나타나요.</span>}
              {blocks.map((b, i) => (
                <span key={i} className="rename__block-wrap">
                  <button
                    type="button"
                    className={`rename__block rename__block--${b.k}`}
                    aria-expanded={menu === i}
                    onClick={(e) => {
                      e.stopPropagation();
                      setMenu(menu === i ? null : i);
                    }}
                  >
                    <small>{preview?.views[i]?.label}</small>
                    <span>{preview?.views[i]?.value}</span>
                  </button>
                  {menu === i && <BlockMenu index={i} blocks={blocks} first={first} onChange={editBlocks} />}
                </span>
              ))}
              <span className="rename__block-wrap">
                <button
                  type="button"
                  className="rename__add"
                  aria-expanded={menu === "add"}
                  onClick={(e) => {
                    e.stopPropagation();
                    setMenu(menu === "add" ? null : "add");
                  }}
                >
                  <Plus size={14} aria-hidden="true" />
                  넣기
                </button>
                {menu === "add" && (
                  <AddMenu
                    first={first}
                    listWord={listWord}
                    onAdd={(b) => {
                      if (b.k === "student") setRosterOpen(true);
                      editBlocks([...blocks, b]);
                    }}
                  />
                )}
              </span>
            </div>
          </div>

          {preview?.hint && <p className="rename__hint">{preview.hint}</p>}

          {showRoster && (
            <div className="rename__roster">
              <div className="rename__roster-head">
                <label htmlFor="rename-roster">
                  {listWord}
                  <span className="rename__muted">
                    {" "}
                    · 엑셀이나 나이스에서 번호와 이름 두 칸을 복사해 붙여 넣으세요. 저장하지 않고, 이 화면을 떠나면 지워져요.
                  </span>
                </label>
                <div className="button-row">
                  {!roster && (
                    <button type="button" className="btn btn--small" onClick={() => setRoster(SAMPLE_ROSTER)}>
                      예시 넣기
                    </button>
                  )}
                  {roster && (
                    <button type="button" className="btn btn--small" onClick={() => setRoster("")}>
                      지우기
                    </button>
                  )}
                </div>
              </div>
              <textarea
                id="rename-roster"
                className="rename__roster-text"
                rows={5}
                placeholder={"1\t김민지\n2\t이서준"}
                value={roster}
                onChange={(e) => setRoster(e.target.value)}
              />
              {roster.trim() && preview && (
                <p className="rename__muted">
                  {preview.roster.count}명 읽음
                  {preview.roster.ignored > 0 && ` · 읽지 못한 줄 ${preview.roster.ignored}개`}
                  {preview.roster.duplicates.length > 0 && ` · 두 번 나온 번호 ${preview.roster.duplicates.join(", ")}(뒤의 이름을 씀)`}
                </p>
              )}
            </div>
          )}
        </section>
      )}

      {/* ③ 미리보기 */}
      {folder && checked.length > 0 && (
        <section className="rename__card" aria-labelledby="rename-step3">
          <h2 className="rename__step" id="rename-step3">
            <span className="rename__num">3</span>미리보기
            {rows.length > 0 && (
              <span className="rename__summary">
                바뀜 {counts.change} · 그대로 {counts.same}
                {counts.skip > 0 && <span className="rename__bad"> · 건너뜀 {counts.skip}</span>}
              </span>
            )}
          </h2>
          <div className="rename__table" role="table" aria-label="바뀔 이름">
            {rows.map((r, i) => {
              const sub = pathToFile.get(r.path)?.subDir;
              return (
                <div key={r.path} role="row" className={`rename__row rename__row--${r.status}${i === 0 ? " rename__row--first" : ""}`}>
                  <span role="cell" className="rename__oldname" title={r.oldName}>
                    {sub && <span className="rename__sub">{sub}</span>}
                    {r.oldName}
                  </span>
                  <ArrowRight size={14} className="rename__arrow" aria-hidden="true" />
                  <span role="cell" className="rename__new" title={r.newName}>
                    {r.status === "same" || r.status === "invalid" || r.status === "skip" ? (
                      <span className="rename__keep">{r.note ?? "그대로"}</span>
                    ) : (
                      r.newName
                    )}
                  </span>
                  <span role="cell" className="rename__state">
                    {r.status === "conflict" ? r.note : STATUS_TEXT[r.status]}
                  </span>
                </div>
              );
            })}
          </div>
        </section>
      )}

      {folder && checked.length > 0 && (
        <div className="rename__bar">
          {toast ? (
            <div className="rename__toast" role="status">
              <span className="rename__toast-text">
                {toast.message}
                {toast.errors.length > 0 && ` · ${toast.errors.length}개는 바꾸지 못했어요(열려 있는 파일 등)`}
              </span>
              {toast.batch !== null && (
                <button type="button" className="rename__undo" onClick={() => void undo()} disabled={busy}>
                  되돌리기
                </button>
              )}
              <button type="button" className="rename__close" aria-label="알림 닫기" onClick={() => setToast(null)}>
                <X size={16} />
              </button>
            </div>
          ) : (
            <span className="rename__bar-text">
              {preview && preview.changes > 0
                ? `${preview.changes}개 파일 이름이 바뀌어요. 바꾼 뒤에도 되돌릴 수 있어요.`
                : "바뀌는 파일이 없어요."}
            </span>
          )}
          <button
            type="button"
            className="btn btn--accent btn--main"
            disabled={busy || !preview || preview.changes === 0}
            onClick={() => void apply()}
          >
            {preview && preview.changes > 0 ? `${preview.changes}개 이름 바꾸기` : "이름 바꾸기"}
          </button>
        </div>
      )}
      {toast && toast.errors.length > 0 && (
        <ul className="rename__errors">
          {toast.errors.map((e) => (
            <li key={e}>{e}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** 블록 하나를 바꾸는 메뉴: 종류 바꾸기 · 글자 고치기 · 순서 옮기기 · 빼기 */
function BlockMenu({
  index,
  blocks,
  first,
  onChange,
}: {
  index: number;
  blocks: Block[];
  first: FirstFile;
  onChange: (next: Block[]) => void;
}) {
  const b = blocks[index];
  const [text, setText] = useState(b.k === "text" ? b.text : "");
  const replace = (nb: Block) => onChange(blocks.map((x, j) => (j === index ? nb : x)));
  const move = (to: number) => {
    const next = [...blocks];
    next.splice(to, 0, next.splice(index, 1)[0]);
    onChange(next);
  };
  const fixedText = b.k === "year" ? first.year : b.k === "today" ? first.today : b.k === "fileDate" ? first.fileDate : b.k === "seq" ? String(b.start).padStart(b.width, "0") : null;
  return (
    <div className="rename__menu" role="menu" onClick={(e) => e.stopPropagation()}>
      {b.k === "text" && (
        <form
          className="rename__menu-row"
          onSubmit={(e) => {
            e.preventDefault();
            if (text) replace({ k: "text", text });
            else onChange(blocks.filter((_, j) => j !== index));
          }}
        >
          <input autoFocus aria-label="글자 고치기" value={text} onChange={(e) => setText(e.target.value)} />
          <button type="submit">고치기</button>
        </form>
      )}
      {b.k === "text" && /^\d+$/.test(b.text) && (
        <button type="button" onClick={() => replace({ k: "seq", start: Number(b.text), width: b.text.length })}>
          순서 번호로({b.text}부터 1씩)
        </button>
      )}
      {fixedText !== null && (
        <button type="button" onClick={() => replace({ k: "text", text: fixedText })}>
          글자 그대로 “{fixedText}”(모든 파일 같게)
        </button>
      )}
      {b.k === "year" && (
        <button type="button" onClick={() => replace({ k: "fileDate" })}>
          파일 날짜로({first.fileDate})
        </button>
      )}
      {b.k === "num" && first.number && (
        <button type="button" onClick={() => replace({ k: "num", plain: !b.plain })}>
          {b.plain ? `앞의 0 그대로(${first.number})` : `앞의 0 빼기(${Number(first.number)})`}
        </button>
      )}
      {b.k === "name" && b.from && (
        <button type="button" onClick={() => replace({ k: "name" })}>
          지금 이름 그대로(바꾸지 않기)
        </button>
      )}
      <hr />
      {index > 0 && (
        <button type="button" onClick={() => move(index - 1)}>
          ← 앞으로
        </button>
      )}
      {index < blocks.length - 1 && (
        <button type="button" onClick={() => move(index + 1)}>
          뒤로 →
        </button>
      )}
      <button type="button" className="rename__menu-danger" onClick={() => onChange(blocks.filter((_, j) => j !== index))}>
        이 블록 빼기
      </button>
    </div>
  );
}

/** 블록 더하기 메뉴 */
function AddMenu({ first, listWord, onAdd }: { first: FirstFile; listWord: string; onAdd: (b: Block) => void }) {
  const [text, setText] = useState("");
  return (
    <div className="rename__menu" role="menu" onClick={(e) => e.stopPropagation()}>
      <form
        className="rename__menu-row"
        onSubmit={(e) => {
          e.preventDefault();
          if (text) onAdd({ k: "text", text });
        }}
      >
        <input autoFocus aria-label="넣을 글자" placeholder="넣을 글자" value={text} onChange={(e) => setText(e.target.value)} />
        <button type="submit">넣기</button>
      </form>
      <hr />
      <button type="button" onClick={() => onAdd({ k: "name" })}>
        지금 이름({first.stem})
      </button>
      <button type="button" onClick={() => onAdd({ k: "year" })}>
        올해({first.year})
      </button>
      <button type="button" onClick={() => onAdd({ k: "today" })}>
        오늘 날짜({first.today})
      </button>
      <button type="button" onClick={() => onAdd({ k: "fileDate" })}>
        파일 날짜({first.fileDate})
      </button>
      <button type="button" onClick={() => onAdd({ k: "seq", start: 1, width: 1 })}>
        순서 번호(1, 2, 3 …)
      </button>
      {first.number && (
        <button type="button" onClick={() => onAdd({ k: "num", plain: false })}>
          이름의 번호({first.number})
        </button>
      )}
      <button type="button" onClick={() => onAdd({ k: "student" })}>
        {listWord}의 이름(번호로 찾기)
      </button>
    </div>
  );
}
