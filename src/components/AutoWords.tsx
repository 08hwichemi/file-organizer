import { useEffect, useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { errorMessage, getAutoWords, setAutoWords } from "../lib/backend";
import type { WordGroup } from "../lib/types";
import Dialog from "./Dialog";
import FolderIcon from "./FolderIcon";

interface Props {
  rootName: string;
  /** 아래 알림(되돌리기 포함) */
  onNotice: (n: { message: string; undo?: () => Promise<void> }) => void;
}

/** "업무 › 공문", "업무/공문", "업무\공문" 모두 받는다 */
const parseFolder = (text: string) =>
  text
    .split(/[›/\\>]/)
    .map((n) => n.trim())
    .filter(Boolean);

/**
 * 자동 분류 단어(SPEC 4-1): 이름에 이 단어가 있으면 그 폴더로. 정리 규칙 화면 아래쪽.
 * 고치고·더하고·빼고, 기본 단어로 되돌릴 수 있다(모두 되돌리기 가능).
 */
export default function AutoWords({ rootName, onNotice }: Props) {
  const [groups, setGroups] = useState<WordGroup[] | null>(null);
  const [editing, setEditing] = useState<number | "new" | null>(null);
  const [folderText, setFolderText] = useState("");
  const [wordsText, setWordsText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    getAutoWords()
      .then(setGroups)
      .catch((e) => onNotice({ message: errorMessage(e) }));
  }, [onNotice]);

  /** 저장하고, 알림에서 바로 앞 상태로 되돌릴 수 있게 한다 */
  const save = async (next: WordGroup[] | null, message: string) => {
    const before = groups;
    setBusy(true);
    try {
      setGroups(await setAutoWords(next));
      onNotice({
        message,
        undo: before
          ? async () => {
              setGroups(await setAutoWords(before));
              onNotice({ message: "되돌렸어요" });
            }
          : undefined,
      });
      return true;
    } catch (e) {
      setError(errorMessage(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const open = (which: number | "new") => {
    const g = which === "new" ? null : groups?.[which];
    setFolderText(g ? g.folder.join(" › ") : "");
    setWordsText(g ? g.words.join(", ") : "");
    setError(null);
    setEditing(which);
  };

  const submit = async () => {
    if (!groups || editing === null) return;
    const group = { folder: parseFolder(folderText), words: wordsText.split(",").map((w) => w.trim()) };
    const next = editing === "new" ? [...groups, group] : groups.map((g, i) => (i === editing ? group : g));
    if (await save(next, editing === "new" ? "단어 묶음을 더했어요" : "자동 분류 단어를 고쳤어요")) setEditing(null);
  };

  return (
    <section className="rules__section">
      <div className="rules__head">
        <div>
          <h2 className="rules__subtitle">자동 분류 단어</h2>
          <p className="page__lead">
            규칙이 없을 때 ‘자동 분류’가 쓰는 단어예요. 파일 이름에 단어가 있으면 그 폴더로 넣어요. 같은 이름의 폴더가 정리
            폴더 안에 있으면 거기로, 없으면 새로 만들어요. 정리 폴더에 있는 폴더 이름도 저절로 기준이 되고, 이름으로 모르면
            ‘형식별’ 폴더(한글 문서·PDF·사진…)로 가요. 설치 파일처럼 형식도 모르는 파일은 정리함에 남아요.
          </p>
        </div>
        <div className="button-row rules__head-actions">
          <button
            type="button"
            className="btn"
            onClick={() => void save(null, "기본 단어로 되돌렸어요")}
            disabled={busy || !groups}
          >
            기본 단어로
          </button>
          <button type="button" className="btn btn--accent" onClick={() => open("new")} disabled={!groups}>
            <Plus size={16} aria-hidden="true" />
            단어 묶음
          </button>
        </div>
      </div>

      {groups && groups.length > 0 && (
        <ul className="rules__list">
          {groups.map((g, i) => (
            <li key={`${g.folder.join("/")}-${i}`} className="rules__item">
              <div className="rules__body">
                <div className="rules__dest rules__dest--strong">
                  <FolderIcon size={16} />
                  <span>{[rootName, ...g.folder].join(" › ")}</span>
                </div>
                <div className="rules__words">
                  {g.words.map((w) => (
                    <span key={w} className="rules__word">
                      {w}
                    </span>
                  ))}
                </div>
              </div>
              <div className="button-row rules__actions">
                <button type="button" className="btn" onClick={() => open(i)}>
                  고치기
                </button>
                <button
                  type="button"
                  className="icon-btn rules__delete"
                  aria-label="단어 묶음 빼기"
                  title="단어 묶음 빼기(되돌리기 가능)"
                  disabled={busy}
                  onClick={() =>
                    void save(
                      groups.filter((_, j) => j !== i),
                      `‘${g.folder.join(" › ")}’ 단어 묶음을 뺐어요`,
                    )
                  }
                >
                  <Trash2 size={16} />
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
      {groups && groups.length === 0 && (
        <div className="rules__empty">
          <p>단어 묶음이 없어요. 폴더 이름과 파일 형식으로만 나눠요.</p>
        </div>
      )}

      {editing !== null && (
        <Dialog
          title={editing === "new" ? "단어 묶음 더하기" : "단어 묶음 고치기"}
          onClose={() => setEditing(null)}
          actions={
            <>
              <button type="button" className="btn btn--ghost" onClick={() => setEditing(null)}>
                취소
              </button>
              <button type="button" className="btn btn--primary" onClick={() => void submit()} disabled={busy}>
                저장
              </button>
            </>
          }
        >
          <label className="rules__field">
            <span>넣을 폴더(정리 폴더 안)</span>
            <input
              type="text"
              autoFocus
              value={folderText}
              placeholder="예: 업무 › 출장"
              onChange={(e) => setFolderText(e.target.value)}
            />
            <span className="rules__hint">› 나 / 로 나눠 적어요. 같은 이름의 폴더가 이미 있으면 그 폴더에 넣어요.</span>
          </label>
          <label className="rules__field">
            <span>이 단어가 이름에 있으면</span>
            <input
              type="text"
              value={wordsText}
              placeholder="예: 출장, 여비, 복명서"
              onChange={(e) => setWordsText(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void submit()}
            />
            <span className="rules__hint">쉼표로 나눠 적어요. 하나만 들어 있어도 맞아요.</span>
          </label>
          {error && <p className="error-text">{error}</p>}
        </Dialog>
      )}
    </section>
  );
}
