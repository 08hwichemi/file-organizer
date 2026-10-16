import { useCallback, useEffect, useState } from "react";
import { ArrowDown, ArrowUp, Plus, Trash2 } from "lucide-react";
import AutoWords from "../components/AutoWords";
import FolderIcon from "../components/FolderIcon";
import RuleEditor from "../components/RuleEditor";
import { deleteRule, errorMessage, listRules, moveRule, onBackendEvent, restoreRule } from "../lib/backend";
import { baseName } from "../lib/paths";
import { ruleCondition, segmentsLabel, splitKinds } from "../lib/rules";
import type { RuleView } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import "./RulesPage.css";

interface Notice {
  message: string;
  undo?: () => Promise<void>;
}

// 정리 규칙 목록(SPEC 3-3·4장). 규칙은 팝업에서 만들고, 여기서는 순서를 바꾸고 고치고 지운다.
export default function RulesPage() {
  const settings = useAppStore((s) => s.settings);
  const known = useAppStore((s) => s.setupInfo);
  const root = settings?.rootFolder ?? null;
  const sep = known?.separator ?? "\\";

  const [rules, setRules] = useState<RuleView[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [editing, setEditing] = useState<RuleView | "new" | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = useCallback(() => {
    listRules()
      .then((r) => {
        setRules(r);
        setLoadError(null);
      })
      .catch((e) => setLoadError(errorMessage(e)));
  }, []);

  useEffect(() => {
    reload();
    return onBackendEvent("inbox-changed", reload);
  }, [reload]);

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 10_000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const run = async (work: () => Promise<Notice | null>) => {
    if (busy) return;
    setBusy(true);
    try {
      const n = await work();
      if (n) setNotice(n);
    } catch (e) {
      setNotice({ message: errorMessage(e) });
    } finally {
      setBusy(false);
      reload();
    }
  };

  const remove = (rule: RuleView) =>
    run(async () => {
      await deleteRule(rule.id);
      return {
        message: `‘${ruleCondition(rule, known)}’ 규칙을 지웠어요`,
        undo: async () => {
          await restoreRule(rule.id);
          setNotice({ message: "규칙을 되돌렸어요" });
        },
      };
    });

  const shift = (rule: RuleView, up: boolean) =>
    run(async () => {
      await moveRule(rule.id, up);
      return null;
    });

  if (!root) return null;
  const rootName = baseName(root);

  return (
    <div className="page rules">
      <div className="rules__head">
        <div>
          <h1 className="page__title">정리 규칙</h1>
          <p className="page__lead">
            새 파일이 오면 위에서부터 먼저 맞는 규칙 하나로 추천 위치를 정해요. 이름 단어가 있는 규칙을 먼저 보고,
            확장자만 있는 규칙은 마지막에 봐요. 직접 만든 규칙이 아래 ‘자동 분류 단어’보다 먼저예요.
          </p>
        </div>
        <button type="button" className="btn btn--accent rules__new" onClick={() => setEditing("new")}>
          <Plus size={16} aria-hidden="true" />
          새 규칙
        </button>
      </div>

      {loadError && <p className="error-text">{loadError}</p>}

      {rules && rules.length === 0 && (
        <div className="rules__empty">
          <div className="rules__empty-title">아직 규칙이 없어요</div>
          <p>
            새 파일 팝업에서 파일을 넣고 나면 ‘다음에도 비슷한 파일은 여기에 넣을까요?’가 나와요. 파일 이름의 단어를
            골라 ‘규칙으로 저장’을 누르면 여기에 생겨요.
          </p>
        </div>
      )}

      {rules && rules.length > 0 && (
        <ol className="rules__list">
          {rules.map((r, i) => (
            <li key={r.id} className={`rules__item${r.missing ? " rules__item--missing" : ""}`}>
              <span className="rules__num" aria-label={`${i + 1}번째`}>
                {i + 1}
              </span>
              <div className="rules__body">
                <div className="rules__cond">{ruleCondition(r, known)}</div>
                <div className="rules__dest">
                  <FolderIcon size={16} />
                  <span title={r.destDir}>{segmentsLabel(rootName, r.segments, r.split)}</span>
                </div>
                <div className="rules__meta">
                  {r.missing ? (
                    <span className="rules__missing">저장 위치 폴더가 없어요. ‘고치기’에서 다시 골라 주세요.</span>
                  ) : (
                    <>
                      {r.used > 0 ? `${r.used}번 정리했어요` : "아직 이 규칙으로 넣은 파일이 없어요"}
                      {r.split && splitKinds(r.segments) && ` · ${splitKinds(r.segments)} 폴더는 파일마다 나눠요`}
                    </>
                  )}
                </div>
              </div>
              <div className="button-row rules__actions">
                <button
                  type="button"
                  className="icon-btn"
                  aria-label="위로"
                  title="위로(먼저 보기)"
                  onClick={() => void shift(r, true)}
                  disabled={busy || i === 0}
                >
                  <ArrowUp size={16} />
                </button>
                <button
                  type="button"
                  className="icon-btn"
                  aria-label="아래로"
                  title="아래로"
                  onClick={() => void shift(r, false)}
                  disabled={busy || i === rules.length - 1}
                >
                  <ArrowDown size={16} />
                </button>
                <button type="button" className="btn" onClick={() => setEditing(r)}>
                  고치기
                </button>
                <button
                  type="button"
                  className="icon-btn rules__delete"
                  aria-label="규칙 지우기"
                  title="규칙 지우기(되돌리기 가능)"
                  onClick={() => void remove(r)}
                  disabled={busy}
                >
                  <Trash2 size={16} />
                </button>
              </div>
            </li>
          ))}
        </ol>
      )}

      <AutoWords rootName={rootName} onNotice={setNotice} />

      {notice && (
        <div className="rules__toast" role="status">
          <span className="rules__toast-text">{notice.message}</span>
          {notice.undo && (
            <button
              type="button"
              className="rules__undo"
              onClick={() => void run(async () => (await notice.undo?.(), null))}
              disabled={busy}
            >
              되돌리기
            </button>
          )}
        </div>
      )}

      {editing && (
        <RuleEditor
          rule={editing === "new" ? null : editing}
          root={root}
          separator={sep}
          known={known}
          watchFolders={settings?.watchFolders ?? []}
          onClose={() => setEditing(null)}
          onSaved={(message) => {
            setNotice({ message });
            reload();
          }}
        />
      )}
    </div>
  );
}
