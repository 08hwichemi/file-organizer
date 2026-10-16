import { useEffect, useMemo, useState } from "react";
import { classifyPreview, errorMessage } from "../lib/backend";
import { displayPath } from "../lib/paths";
import { reasonLabel } from "../lib/rules";
import type { ClassifyPlan } from "../lib/types";
import Dialog from "./Dialog";
import FolderIcon from "./FolderIcon";
import "./ClassifyDialog.css";

interface Props {
  /** 나눌 파일(정리함에서 지금 보이는 파일) */
  paths: string[];
  root: string;
  separator: string;
  onClose: () => void;
  /** 체크한 파일을 나눈다 */
  onRun: (paths: string[]) => void;
}

/**
 * 자동 분류 미리보기(SPEC 4-1): 파일 이름·형식으로 정한 폴더별로 보여 주고, 확인하면 한꺼번에 넣는다.
 * 구별이 안 되는 파일은 정리함에 그대로 남는다.
 */
export default function ClassifyDialog({ paths, root, separator, onClose, onRun }: Props) {
  const [plans, setPlans] = useState<ClassifyPlan[] | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    classifyPreview(paths)
      .then((p) => {
        if (!alive) return;
        setPlans(p);
        setChecked(new Set(p.filter((x) => x.target).map((x) => x.path)));
      })
      .catch((e) => alive && setError(errorMessage(e)));
    return () => {
      alive = false;
    };
  }, [paths]);

  // 넣을 폴더별로 묶기(파일이 많은 폴더부터)
  const groups = useMemo(() => {
    const map = new Map<string, ClassifyPlan[]>();
    for (const p of plans ?? []) {
      if (!p.target) continue;
      const list = map.get(p.target.destDir) ?? [];
      list.push(p);
      map.set(p.target.destDir, list);
    }
    return [...map.entries()].sort((a, b) => b[1].length - a[1].length);
  }, [plans]);
  const unknown = (plans ?? []).filter((p) => !p.target);

  const toggle = (path: string) =>
    setChecked((c) => {
      const next = new Set(c);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  return (
    <Dialog
      title="자동 분류"
      onClose={onClose}
      className="dialog--classify"
      actions={
        <>
          <button type="button" className="btn btn--ghost" onClick={onClose}>
            취소
          </button>
          <button
            type="button"
            className="btn btn--primary"
            onClick={() => onRun([...checked])}
            disabled={checked.size === 0}
          >
            {checked.size}개 나누기
          </button>
        </>
      }
    >
      <p className="dialog__sub">
        파일 이름과 형식을 보고 이렇게 나눌게요. 빼고 싶은 파일은 체크를 끄세요. 없는 폴더는 새로 만들고, 나눈 뒤에도
        되돌릴 수 있어요.
      </p>
      {error && <p className="error-text">{error}</p>}
      {!plans && !error && <p className="dialog__sub">살펴보는 중…</p>}
      {plans && (
        <div className="classify">
          {groups.map(([dest, list]) => (
            <section key={dest} className="classify__group">
              <div className="classify__head">
                <FolderIcon size={20} />
                <span className="classify__path" title={dest}>
                  {displayPath(root, dest, separator)}
                </span>
                {!list[0].target?.exists && <span className="classify__new">새 폴더</span>}
                <span className="classify__count">{list.length}개</span>
              </div>
              <ul className="classify__files">
                {list.map((p) => (
                  <li key={p.path}>
                    <label className="classify__file">
                      <input type="checkbox" checked={checked.has(p.path)} onChange={() => toggle(p.path)} />
                      <span className="classify__name" title={p.name}>
                        {p.name}
                      </span>
                      <span className="classify__why">{p.target && reasonLabel(p.target.reason)}</span>
                    </label>
                  </li>
                ))}
              </ul>
            </section>
          ))}
          {unknown.length > 0 && (
            <section className="classify__group classify__group--unknown">
              <div className="classify__head">
                <span className="classify__path">구별이 안 되는 파일 {unknown.length}개</span>
              </div>
              <p className="classify__note">이름과 형식으로 넣을 곳을 정하지 못했어요. 정리함에 그대로 남아요.</p>
              <ul className="classify__files">
                {unknown.map((p) => (
                  <li key={p.path} className="classify__file classify__file--off">
                    <span className="classify__name" title={p.name}>
                      {p.name}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          )}
          {plans.length === 0 && <p className="dialog__sub">나눌 파일이 없어요.</p>}
        </div>
      )}
    </Dialog>
  );
}
