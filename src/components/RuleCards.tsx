import { useState } from "react";
import { answerRuleChange, createRule, errorMessage } from "../lib/backend";
import { withRo } from "../lib/format";
import { baseName, displayPath } from "../lib/paths";
import { ruleCondition, ruleName, segmentsLabel, splitKinds } from "../lib/rules";
import type { Rule, RuleChange, RuleDraft } from "../lib/types";
import WordMatchChoice from "./WordMatchChoice";
import "./RuleCards.css";

interface DraftProps {
  draft: RuleDraft;
  root: string;
  separator: string;
  /** "이번만" = 규칙 없이 닫기 */
  onSkip: () => void;
}

/**
 * 넣은 뒤 "다음에도 비슷한 파일은 여기에 넣을까요?"(SPEC 3-1 ⑥, 시안 popup.dc.html).
 * 파일 이름의 단어를 눌러 조건으로 고르고, 연도·학기 폴더는 파일마다 나눌 수 있다.
 */
export function RuleDraftCard({ draft, root, separator, onSkip }: DraftProps) {
  const [chosen, setChosen] = useState<string[]>(draft.preselected);
  const [anyWord, setAnyWord] = useState(false);
  const kinds = splitKinds(draft.segments);
  const [split, setSplit] = useState(kinds !== "");
  const [saved, setSaved] = useState<Rule | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const rootName = baseName(root);
  const useSplit = split && kinds !== "";
  const dest = useSplit ? segmentsLabel(rootName, draft.segments, true) : displayPath(root, draft.destDir, separator);
  const condition =
    chosen.length === 0 ? "기준 단어를 하나 이상 골라 주세요" : ruleCondition({ words: chosen, anyWord: anyWord && chosen.length > 1, extensions: [], sourceDir: null }, null);

  const toggle = (w: string) => setChosen((c) => (c.includes(w) ? c.filter((x) => x !== w) : [...c, w]));

  const save = async () => {
    if (chosen.length === 0 || busy) return;
    setBusy(true);
    setError(null);
    try {
      // 고른 순서가 아니라 이름에 나온 순서로
      const words = draft.words.filter((w) => chosen.includes(w));
      setSaved(
        await createRule({
          words,
          anyWord: anyWord && words.length > 1,
          extensions: [],
          sourceDir: null,
          destDir: draft.destDir,
          split: useSplit,
        }),
      );
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (saved) {
    return (
      <div className="rule-card rule-card--done" role="status">
        <div className="rule-card__title rule-card__title--done">규칙을 저장했어요</div>
        <div className="rule-card__line">
          {ruleCondition(saved, null)} → {dest}
        </div>
        <div className="rule-card__sub">
          다음부터는 추천 위치로 먼저 보여 드려요. 규칙은 프로그램의 ‘정리 규칙’에서 언제든 고칠 수 있어요.
        </div>
      </div>
    );
  }

  return (
    <div className="rule-card">
      <div>
        <div className="rule-card__title">다음에도 비슷한 파일은 여기에 넣을까요?</div>
        <div className="rule-card__sub">파일 이름에서 기준이 될 단어를 골라 주세요.</div>
      </div>
      {draft.words.length > 0 ? (
        <div className="rule-card__words" role="group" aria-label="기준 단어">
          {draft.words.map((w) => {
            const on = chosen.includes(w);
            return (
              <button
                key={w}
                type="button"
                className={`rule-card__word${on ? " rule-card__word--on" : ""}`}
                aria-pressed={on}
                onClick={() => toggle(w)}
              >
                {w}
              </button>
            );
          })}
        </div>
      ) : (
        <div className="rule-card__sub">이름에서 고를 단어가 없어요. ‘정리 규칙’에서 확장자로 규칙을 만들 수 있어요.</div>
      )}
      {chosen.length > 1 && <WordMatchChoice anyWord={anyWord} onChange={setAnyWord} />}
      {kinds !== "" && (
        <label className="rule-card__check">
          <input type="checkbox" checked={split} onChange={(e) => setSplit(e.target.checked)} />
          <span>{kinds} 폴더는 파일마다 알아서 나누기</span>
        </label>
      )}
      <dl className="rule-card__preview">
        <div>
          <dt>조건</dt>
          <dd>{condition}</dd>
        </div>
        <div>
          <dt>저장</dt>
          <dd>{dest}</dd>
        </div>
        {useSplit && draft.exampleLabel && (
          <div className="rule-card__example">
            <dt />
            <dd>
              예) {draft.exampleLabel} 파일은 {withRo([rootName, ...draft.exampleSegments].join(" › "))} 가요
            </dd>
          </div>
        )}
      </dl>
      {error && <p className="error-text">{error}</p>}
      <div className="button-row rule-card__actions">
        <button type="button" className="btn btn--ghost" onClick={onSkip}>
          이번만
        </button>
        <button type="button" className="btn btn--accent" onClick={() => void save()} disabled={chosen.length === 0 || busy}>
          규칙으로 저장
        </button>
      </div>
    </div>
  );
}

interface ChangeProps {
  change: RuleChange;
  root: string;
  separator: string;
}

/** 같은 규칙에서 같은 다른 폴더를 세 번 연속 고르면: "규칙을 바꿀까요?"(SPEC 4장) */
export function RuleChangeCard({ change, root, separator }: ChangeProps) {
  const [answer, setAnswer] = useState<"changed" | "kept" | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const to = displayPath(root, change.newDir, separator);

  const reply = async (destDir: string | null) => {
    setBusy(true);
    setError(null);
    try {
      await answerRuleChange(change.rule.id, destDir);
      setAnswer(destDir ? "changed" : "kept");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (answer === "kept") return null;
  if (answer === "changed") {
    return (
      <div className="rule-card rule-card--done" role="status">
        <div className="rule-card__title rule-card__title--done">규칙을 바꿨어요</div>
        <div className="rule-card__line">
          다음부터 {ruleName(change.rule)}은 {withRo(to)} 추천해요.
        </div>
      </div>
    );
  }
  return (
    <div className="rule-card">
      <div>
        <div className="rule-card__title">규칙을 바꿀까요?</div>
        <div className="rule-card__sub">
          {ruleName(change.rule)}에 맞는 파일을 세 번 연속 다른 폴더에 넣으셨어요.
        </div>
      </div>
      <dl className="rule-card__preview">
        <div>
          <dt>지금</dt>
          <dd>{displayPath(root, change.oldDir, separator)}</dd>
        </div>
        <div>
          <dt>바꾸면</dt>
          <dd>{to}</dd>
        </div>
      </dl>
      {error && <p className="error-text">{error}</p>}
      <div className="button-row rule-card__actions">
        <button type="button" className="btn btn--ghost" onClick={() => void reply(null)} disabled={busy}>
          그대로 두기
        </button>
        <button type="button" className="btn btn--accent" onClick={() => void reply(change.newDir)} disabled={busy}>
          이 폴더로 바꾸기
        </button>
      </div>
    </div>
  );
}
