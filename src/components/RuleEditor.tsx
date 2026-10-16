import { useEffect, useState } from "react";
import { X } from "lucide-react";
import { createRule, describeDest, errorMessage, updateRule } from "../lib/backend";
import { baseName, displayPath, friendlyName, samePath, type KnownFolders } from "../lib/paths";
import { parseExtensions, segmentsLabel, splitKinds } from "../lib/rules";
import type { DestSegment, RuleView } from "../lib/types";
import Dialog from "./Dialog";
import FolderPicker from "./FolderPicker";
import "./RuleEditor.css";

interface Props {
  /** 고칠 규칙(없으면 새 규칙) */
  rule: RuleView | null;
  root: string;
  separator: string;
  known: KnownFolders | null;
  watchFolders: string[];
  onClose: () => void;
  onSaved: (message: string) => void;
}

/** 규칙 만들기·고치기 창(SPEC 4장): 이름 단어 · 확장자 · 출처 → 넣을 폴더(연도·학기 나누기) */
export default function RuleEditor({ rule, root, separator, known, watchFolders, onClose, onSaved }: Props) {
  const [words, setWords] = useState<string[]>(rule?.words ?? []);
  const [wordDraft, setWordDraft] = useState("");
  const [exts, setExts] = useState(rule?.extensions.join(", ") ?? "");
  const [source, setSource] = useState(rule?.sourceDir ?? "");
  const [dest, setDest] = useState<string | null>(rule?.destDir ?? null);
  const [split, setSplit] = useState(rule?.split ?? true);
  const [segments, setSegments] = useState<DestSegment[]>(rule?.segments ?? []);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!dest) return;
    let alive = true;
    describeDest(dest)
      .then((s) => alive && setSegments(s))
      .catch(() => alive && setSegments([]));
    return () => {
      alive = false;
    };
  }, [dest]);

  const kinds = dest ? splitKinds(segments) : "";
  const sources = rule?.sourceDir && !watchFolders.some((w) => samePath(w, rule.sourceDir!))
    ? [...watchFolders, rule.sourceDir]
    : watchFolders;

  /** 적고 있던 단어를 더한 단어 목록 */
  const withDraft = (text: string) => {
    const next = [...words];
    for (const w of text.split(",").map((x) => x.trim()).filter(Boolean)) {
      if (!next.some((x) => x.toLowerCase() === w.toLowerCase())) next.push(w);
    }
    return next;
  };

  const addWord = () => {
    setWords(withDraft(wordDraft));
    setWordDraft("");
  };

  const save = async () => {
    if (!dest || busy) return;
    setBusy(true);
    setError(null);
    const input = {
      words: withDraft(wordDraft),
      extensions: parseExtensions(exts),
      sourceDir: source || null,
      destDir: dest,
      split: split && kinds !== "",
    };
    try {
      if (rule) await updateRule(rule.id, input);
      else await createRule(input);
      onSaved(rule ? "규칙을 고쳤어요" : "새 규칙을 만들었어요. 맨 위에 놓였어요.");
      onClose();
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  };

  return (
    <Dialog
      title={rule ? "규칙 고치기" : "새 규칙"}
      onClose={onClose}
      className="dialog--rule"
      actions={
        <>
          <button type="button" className="btn btn--ghost" onClick={onClose}>
            취소
          </button>
          <button type="button" className="btn btn--primary" onClick={() => void save()} disabled={busy || !dest}>
            저장
          </button>
        </>
      }
    >
      <div className="rule-edit">
        <div className="rule-edit__field">
          <label className="rule-edit__label" htmlFor="rule-word">
            파일 이름에 들어 있는 단어
          </label>
          <div className="rule-edit__words">
            {words.map((w) => (
              <span key={w} className="rule-edit__chip">
                {w}
                <button
                  type="button"
                  aria-label={`‘${w}’ 빼기`}
                  onClick={() => setWords((list) => list.filter((x) => x !== w))}
                >
                  <X size={13} />
                </button>
              </span>
            ))}
            <input
              id="rule-word"
              type="text"
              value={wordDraft}
              placeholder={words.length === 0 ? "예: 수행평가" : "단어 더하기"}
              onChange={(e) => {
                const v = e.target.value;
                if (v.endsWith(",")) {
                  setWords(withDraft(v));
                  setWordDraft("");
                } else setWordDraft(v);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addWord();
                } else if (e.key === "Backspace" && !wordDraft && words.length > 0) {
                  setWords((list) => list.slice(0, -1));
                }
              }}
              onBlur={addWord}
            />
          </div>
          <p className="rule-edit__hint">단어를 적고 Enter. 여러 개면 모두 들어 있는 파일만 맞아요.</p>
        </div>

        <div className="rule-edit__pair">
          <div className="rule-edit__field">
            <label className="rule-edit__label" htmlFor="rule-ext">
              확장자
            </label>
            <input
              id="rule-ext"
              type="text"
              className="rule-edit__input"
              value={exts}
              placeholder="예: hwp, hwpx"
              onChange={(e) => setExts(e.target.value)}
            />
            <p className="rule-edit__hint">비워 두면 아무 형식이나</p>
          </div>
          <div className="rule-edit__field">
            <label className="rule-edit__label" htmlFor="rule-source">
              어디서 온 파일
            </label>
            <select id="rule-source" className="rule-edit__input" value={source} onChange={(e) => setSource(e.target.value)}>
              <option value="">어디서 왔든</option>
              {sources.map((w) => (
                <option key={w} value={w}>
                  {friendlyName(w, known)}에서 온 파일만
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className="rule-edit__field">
          <div className="rule-edit__label">넣을 폴더</div>
          <p className="folder-move__target">
            넣을 곳
            <strong>
              {dest
                ? kinds && split
                  ? segmentsLabel(baseName(root), segments, true)
                  : displayPath(root, dest, separator)
                : "아래에서 폴더를 골라 주세요"}
            </strong>
          </p>
          <div className="rule-edit__picker">
            <FolderPicker
              root={root}
              separator={separator}
              selected={dest}
              onSelect={setDest}
              reveal={rule?.destDir ?? null}
            />
          </div>
          {kinds && (
            <label className="rule-edit__check">
              <input type="checkbox" checked={split} onChange={(e) => setSplit(e.target.checked)} />
              <span>{kinds} 폴더는 파일마다 알아서 나누기</span>
            </label>
          )}
          {kinds && split && (
            <p className="rule-edit__hint">
              파일 이름의 연도·학기로 폴더를 골라 넣어요. 이름에 없으면 오늘 날짜로 정하고, 폴더가 없으면 만들어요.
            </p>
          )}
        </div>
        {error && <p className="error-text">{error}</p>}
      </div>
    </Dialog>
  );
}
