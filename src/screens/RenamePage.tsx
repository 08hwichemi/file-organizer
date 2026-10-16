import { useEffect, useMemo, useRef, useState } from "react";
import { ArrowRight, FolderOpen, GripVertical, Plus, X } from "lucide-react";
import FileBadge from "../components/FileBadge";
import { errorMessage, pickFolder, renameApply, renameList, renamePreview, renameUndo } from "../lib/backend";
import { formatDate } from "../lib/format";
import type {
  NumberOptions,
  PieceInfo,
  RenameFile,
  RenamePreview,
  RenameRequest,
  RenameStatus,
  Replacement,
} from "../lib/types";
import "./RenamePage.css";

type Order = "name" | "date" | "manual";

interface Toast {
  message: string;
  batch: number | null;
  errors: string[];
}

/** 틀에 끼워 넣는 칸 */
const SLOTS: { slot: string; hint: string }[] = [
  { slot: "원래 이름", hint: "지금 이름(확장자 빼고)" },
  { slot: "번호", hint: "이름에서 읽거나 순서대로" },
  { slot: "학생 이름", hint: "명렬표에서 번호로 찾음" },
  { slot: "연도", hint: "이름의 2027 등" },
  { slot: "학기", hint: "이름의 1학기·2학기" },
  { slot: "학급", hint: "이름의 3-2 등" },
  { slot: "날짜", hint: "고친 날(2027-10-13)" },
];

const DIGITS = [
  { value: 0, label: "자동" },
  { value: 1, label: "1" },
  { value: 2, label: "01" },
  { value: 3, label: "001" },
];

const STATUS_TEXT: Record<RenameStatus, string> = {
  same: "그대로",
  rename: "바꿈",
  conflict: "번호 붙임",
  invalid: "건너뜀",
  skip: "건너뜀",
};

const byName = (a: RenameFile, b: RenameFile) =>
  a.subDir.localeCompare(b.subDir, "ko", { numeric: true }) || a.name.localeCompare(b.name, "ko", { numeric: true });

/** 조각 칩 글자와 틀에 끼울 칸 */
function pieceChip(p: PieceInfo, numberPiece: number | null): { label: string; slot: string } {
  switch (p.kind) {
    case "class":
      return { label: `학급 ${p.sample}`, slot: "{학급}" };
    case "year":
      return { label: `연도 ${p.sample}`, slot: "{연도}" };
    case "term":
      return { label: `학기 ${p.sample}`, slot: "{학기}" };
    case "number":
      return p.index === numberPiece
        ? { label: `번호 ${p.sample}…`, slot: "{번호}" }
        : { label: `조각 ${p.index} · ${p.sample}`, slot: `{조각 ${p.index}}` };
    default:
      return p.common
        ? { label: `조각 ${p.index} · ‘${p.sample}’`, slot: `{조각 ${p.index}}` }
        : { label: `조각 ${p.index} · ${p.sample}…`, slot: `{조각 ${p.index}}` };
  }
}

/**
 * 이름 한꺼번에 바꾸기(SPEC 3-4). 위에서 아래로 ① 폴더와 파일 → ② 바꾸는 방법 → ③ 미리보기 → [n개 이름 바꾸기].
 * 명렬표는 이 화면의 상태로만 들고 있다가 화면을 떠나면 사라진다(저장하지 않음).
 */
export default function RenamePage() {
  const [folder, setFolder] = useState<string | null>(null);
  const [includeSub, setIncludeSub] = useState(false);
  const [files, setFiles] = useState<RenameFile[]>([]);
  const [truncated, setTruncated] = useState(false);
  const [unchecked, setUnchecked] = useState<Set<string>>(new Set());
  const [order, setOrder] = useState<Order>("name");
  const [manual, setManual] = useState<string[]>([]);
  const [dragging, setDragging] = useState<string | null>(null);

  const [template, setTemplate] = useState("");
  const [number, setNumber] = useState<NumberOptions>({ fromName: true, start: 1, digits: 0 });
  const [replacements, setReplacements] = useState<Replacement[]>([{ find: "", with: "" }]);
  const [prefix, setPrefix] = useState("");
  const [suffix, setSuffix] = useState("");
  const [roster, setRoster] = useState("");

  const [preview, setPreview] = useState<RenamePreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<Toast | null>(null);
  const templateInput = useRef<HTMLInputElement>(null);
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

  const checked = useMemo(() => ordered.filter((f) => !unchecked.has(f.path)).map((f) => f.path), [ordered, unchecked]);

  const request: RenameRequest = useMemo(
    () => ({
      files: checked,
      template,
      number,
      replacements: replacements.filter((r) => r.find !== ""),
      prefix,
      suffix,
      roster,
    }),
    [checked, template, number, replacements, prefix, suffix, roster],
  );

  // 바꿀 때마다 미리보기(잠깐 멈추면)
  useEffect(() => {
    if (checked.length === 0) {
      setPreview(null);
      return;
    }
    const id = ++requestId.current;
    const timer = window.setTimeout(() => {
      renamePreview(request)
        .then((p) => id === requestId.current && setPreview(p))
        .catch((e) => id === requestId.current && setError(errorMessage(e)));
    }, 180);
    return () => window.clearTimeout(timer);
  }, [request, checked.length]);

  const insert = (text: string) => {
    const input = templateInput.current;
    const start = input?.selectionStart ?? template.length;
    const end = input?.selectionEnd ?? template.length;
    const next = template.slice(0, start) + text + template.slice(end);
    setTemplate(next);
    window.requestAnimationFrame(() => {
      input?.focus();
      input?.setSelectionRange(start + text.length, start + text.length);
    });
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
      const result = await renameApply(request);
      setToast({
        message:
          result.renamed > 0 ? `${result.renamed}개 파일 이름을 바꿨어요` : "이름을 바꾼 파일이 없어요",
        batch: result.renamed > 0 ? result.batch : null,
        errors: result.errors,
      });
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
      await load(folder, includeSub);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const usesNumber = template.includes("{번호}") || template.includes("{학생 이름}");
  const usesStudent = template.includes("{학생 이름}");
  const analysis = preview?.analysis ?? null;
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

  return (
    <div className="page rename">
      <div>
        <h1 className="page__title">이름 바꾸기</h1>
        <p className="page__lead">
          폴더를 고르면 그 안 파일 이름을 한꺼번에 바꿔요. 확장자는 그대로 두고, 같은 이름이 있으면 덮어쓰지 않고 (2)를
          붙여요. 바꾼 뒤에도 한 번에 되돌릴 수 있어요.
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
                <select
                  className="rename__select"
                  aria-label="순서"
                  value={order}
                  onChange={(e) => setOrder(e.target.value as Order)}
                >
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
                    <input
                      type="checkbox"
                      aria-label={`${f.name} 고르기`}
                      checked={!unchecked.has(f.path)}
                      onChange={() => toggle(f.path)}
                    />
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

      {/* ② 바꾸는 방법 */}
      {folder && files.length > 0 && (
        <section className="rename__card" aria-labelledby="rename-step2">
          <h2 className="rename__step" id="rename-step2">
            <span className="rename__num">2</span>바꾸는 방법
          </h2>

          {analysis && analysis.pieces.length > 0 && (
            <div className="rename__guess">
              <div className="rename__guess-title">
                알아낸 이름 모양
                <span className="rename__muted">
                  {analysis.matching === analysis.total
                    ? ` · ${analysis.total}개 모두 같은 모양`
                    : ` · ${analysis.total}개 중 ${analysis.matching}개가 이 모양(나머지는 조각을 쓰면 건너뜀)`}
                </span>
              </div>
              <div className="rename__chips">
                {analysis.pieces.map((p) => {
                  const chip = pieceChip(p, analysis.numberPiece);
                  return (
                    <button
                      key={p.index}
                      type="button"
                      className={`rename__chip rename__chip--${p.kind}`}
                      title={`누르면 틀에 ${chip.slot} 넣기`}
                      onClick={() => insert(chip.slot)}
                    >
                      {chip.label}
                    </button>
                  );
                })}
              </div>
              {analysis.suggestions.length > 0 && (
                <div className="rename__suggests">
                  {analysis.suggestions.map((s) => (
                    <button
                      key={s.template}
                      type="button"
                      className={`rename__suggest${template === s.template ? " rename__suggest--on" : ""}`}
                      onClick={() => {
                        setTemplate(s.template);
                        setNumber((n) => ({ ...n, fromName: true }));
                      }}
                    >
                      <span className="rename__suggest-label">{s.label}</span>
                      <code>{s.template}</code>
                      {s.needsRoster && <span className="rename__tag">명렬표 필요</span>}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}

          <div className="rename__block">
            <label className="rename__label" htmlFor="rename-template">
              틀로 새로 짓기
              <span className="rename__muted"> · 비워 두면 지금 이름에서 글자만 바꿔요</span>
            </label>
            <div className="rename__template">
              <input
                id="rename-template"
                ref={templateInput}
                className="rename__input"
                placeholder="예: {학생 이름}({번호})"
                value={template}
                onChange={(e) => setTemplate(e.target.value)}
              />
              {template && (
                <button type="button" className="btn" onClick={() => setTemplate("")}>
                  비우기
                </button>
              )}
            </div>
            <div className="rename__slots" aria-label="칸 끼워 넣기">
              {SLOTS.map((s) => (
                <button key={s.slot} type="button" className="rename__slot" title={s.hint} onClick={() => insert(`{${s.slot}}`)}>
                  <Plus size={13} aria-hidden="true" />
                  {s.slot}
                </button>
              ))}
            </div>
          </div>

          {usesNumber && (
            <div className="rename__block rename__number">
              <span className="rename__label">번호</span>
              <div className="rename__options">
                <label className="rename__check">
                  <input
                    type="radio"
                    name="rename-number"
                    checked={number.fromName}
                    onChange={() => setNumber((n) => ({ ...n, fromName: true }))}
                  />
                  이름에서 읽기
                </label>
                <label className="rename__check">
                  <input
                    type="radio"
                    name="rename-number"
                    checked={!number.fromName}
                    onChange={() => setNumber((n) => ({ ...n, fromName: false }))}
                  />
                  순서대로 매기기
                </label>
                {!number.fromName && (
                  <label className="rename__inline">
                    시작
                    <input
                      type="number"
                      min={0}
                      className="rename__small"
                      value={number.start}
                      onChange={(e) => setNumber((n) => ({ ...n, start: Math.max(0, Number(e.target.value) || 0) }))}
                    />
                  </label>
                )}
                <label className="rename__inline">
                  자릿수
                  <select
                    className="rename__select"
                    value={number.digits}
                    onChange={(e) => setNumber((n) => ({ ...n, digits: Number(e.target.value) }))}
                  >
                    {DIGITS.map((d) => (
                      <option key={d.value} value={d.value}>
                        {d.label}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
            </div>
          )}

          <div className="rename__block">
            <label className="rename__label" htmlFor="rename-roster">
              명렬표{usesStudent ? "" : " (학생 이름을 넣을 때)"}
              <span className="rename__muted"> · 저장하지 않아요. 이 화면을 떠나면 지워져요</span>
            </label>
            <textarea
              id="rename-roster"
              className="rename__roster"
              rows={usesStudent || roster ? 5 : 2}
              placeholder={"나이스·엑셀에서 ‘번호’와 ‘이름’ 두 칸을 복사해 붙여 넣으세요.\n예) 1	김민지"}
              value={roster}
              onChange={(e) => setRoster(e.target.value)}
            />
            {roster.trim() && preview && (
              <p className="rename__muted rename__roster-info">
                학생 {preview.roster.count}명 읽음
                {preview.roster.ignored > 0 && ` · 읽지 못한 줄 ${preview.roster.ignored}개`}
                {preview.roster.duplicates.length > 0 && ` · 두 번 나온 번호 ${preview.roster.duplicates.join(", ")}(뒤의 이름을 씀)`}
                <button type="button" className="rename__link" onClick={() => setRoster("")}>
                  지우기
                </button>
              </p>
            )}
          </div>

          <div className="rename__block">
            <span className="rename__label">
              글자 바꾸기<span className="rename__muted"> · 대소문자 상관없이, 바꿀 글자를 비우면 지워요</span>
            </span>
            {replacements.map((r, i) => (
              <div key={i} className="rename__replace">
                <input
                  className="rename__input"
                  aria-label="찾을 글자"
                  placeholder="찾을 글자"
                  value={r.find}
                  onChange={(e) => setReplacements((list) => list.map((x, j) => (j === i ? { ...x, find: e.target.value } : x)))}
                />
                <ArrowRight size={16} className="rename__arrow" aria-hidden="true" />
                <input
                  className="rename__input"
                  aria-label="바꿀 글자"
                  placeholder="바꿀 글자(비우면 지움)"
                  value={r.with}
                  onChange={(e) => setReplacements((list) => list.map((x, j) => (j === i ? { ...x, with: e.target.value } : x)))}
                />
                <button
                  type="button"
                  className="rename__icon"
                  aria-label="이 줄 빼기"
                  onClick={() =>
                    setReplacements((list) => (list.length === 1 ? [{ find: "", with: "" }] : list.filter((_, j) => j !== i)))
                  }
                >
                  <X size={16} />
                </button>
              </div>
            ))}
            <div className="rename__affix">
              <button
                type="button"
                className="btn btn--small"
                onClick={() => setReplacements((list) => [...list, { find: "", with: "" }])}
              >
                <Plus size={14} aria-hidden="true" />줄 더하기
              </button>
              <label className="rename__inline">
                앞에 붙이기
                <input className="rename__input rename__affix-input" value={prefix} onChange={(e) => setPrefix(e.target.value)} />
              </label>
              <label className="rename__inline">
                뒤에 붙이기
                <input className="rename__input rename__affix-input" value={suffix} onChange={(e) => setSuffix(e.target.value)} />
              </label>
            </div>
          </div>
        </section>
      )}

      {/* ③ 미리보기 */}
      {folder && files.length > 0 && (
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
          {rows.length === 0 ? (
            <p className="rename__empty">위에서 파일을 하나 이상 고르세요.</p>
          ) : (
            <div className="rename__table" role="table" aria-label="바뀔 이름">
              {rows.map((r) => {
                const sub = pathToFile.get(r.path)?.subDir;
                return (
                  <div key={r.path} role="row" className={`rename__row rename__row--${r.status}`}>
                    <span role="cell" className="rename__old" title={r.oldName}>
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
          )}
        </section>
      )}

      {folder && files.length > 0 && (
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
