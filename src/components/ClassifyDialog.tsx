import { useEffect, useMemo, useState } from "react";
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import { GripVertical, RotateCcw } from "lucide-react";
import { classifyPreview, errorMessage } from "../lib/backend";
import { displayPath } from "../lib/paths";
import { reasonLabel } from "../lib/rules";
import type { ClassifyPlan } from "../lib/types";
import Dialog from "./Dialog";
import FileBadge from "./FileBadge";
import FolderIcon from "./FolderIcon";
import FolderPane from "./FolderPane";
import "./ClassifyDialog.css";

/** 직접 옮긴 파일: 넣을 폴더(null = 정리함에 남기기) */
type Overrides = Map<string, string | null>;

interface Props {
  /** 나눌 파일(정리함에서 지금 보이는 파일) */
  paths: string[];
  root: string;
  separator: string;
  onClose: () => void;
  /** auto = 자동 분류대로 넣을 파일, manual = 직접 옮긴 파일(폴더별) */
  onRun: (auto: string[], manual: { destDir: string; paths: string[] }[]) => void;
}

/** 놓을 곳 데이터: 폴더 칸(FolderPane)과 같은 모양 { path }, 정리함에 남기기는 { inbox: true } */
type Drop = { path?: string; inbox?: boolean };

/**
 * 자동 분류 미리보기(SPEC 4-1): 넣을 폴더별로 보여 주고, 확인하면 한꺼번에 넣는다.
 * 넓은 창에서 왼쪽 파일 목록은 따로 내려가고, 오른쪽 폴더 칸은 제자리에 있어서
 * 파일을 끌어 다른 폴더(또는 다른 묶음·정리함에 남기기)에 놓으면 그 파일만 넣을 곳이 바뀐다.
 */
export default function ClassifyDialog({ paths, root, separator, onClose, onRun }: Props) {
  const [plans, setPlans] = useState<ClassifyPlan[] | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [overrides, setOverrides] = useState<Overrides>(new Map());
  const [dragging, setDragging] = useState<ClassifyPlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 5 } }));

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

  /** 이 파일을 넣을 곳(직접 옮겼으면 그곳) */
  const destOf = (p: ClassifyPlan): string | null => (overrides.has(p.path) ? (overrides.get(p.path) ?? null) : (p.target?.destDir ?? null));

  // 넣을 폴더별로 묶기(파일이 많은 폴더부터)
  const groups = useMemo(() => {
    const map = new Map<string, ClassifyPlan[]>();
    for (const p of plans ?? []) {
      const dest = overrides.has(p.path) ? overrides.get(p.path) : p.target?.destDir;
      if (!dest) continue;
      map.set(dest, [...(map.get(dest) ?? []), p]);
    }
    return [...map.entries()].sort((a, b) => b[1].length - a[1].length);
  }, [plans, overrides]);
  const left = (plans ?? []).filter((p) => destOf(p) === null);

  const toggle = (path: string) =>
    setChecked((c) => {
      const next = new Set(c);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const moveTo = (plan: ClassifyPlan, dest: string | null) => {
    const original = plan.target?.destDir ?? null;
    setOverrides((o) => {
      const next = new Map(o);
      if (dest === original) next.delete(plan.path);
      else next.set(plan.path, dest);
      return next;
    });
    setChecked((c) => {
      const next = new Set(c);
      if (dest) next.add(plan.path);
      else next.delete(plan.path);
      return next;
    });
  };

  const onDragStart = (e: DragStartEvent) => setDragging((e.active.data.current as { plan: ClassifyPlan }).plan);
  const onDragEnd = (e: DragEndEvent) => {
    setDragging(null);
    const plan = (e.active.data.current as { plan: ClassifyPlan } | undefined)?.plan;
    const drop = e.over?.data.current as Drop | undefined;
    if (!plan || !drop) return;
    if (drop.inbox) moveTo(plan, null);
    else if (drop.path) moveTo(plan, drop.path);
  };

  const run = () => {
    const auto: string[] = [];
    const manual = new Map<string, string[]>();
    for (const p of plans ?? []) {
      if (!checked.has(p.path)) continue;
      if (overrides.has(p.path)) {
        const dest = overrides.get(p.path);
        if (dest) manual.set(dest, [...(manual.get(dest) ?? []), p.path]);
      } else if (p.target) auto.push(p.path);
    }
    onRun(
      auto,
      [...manual.entries()].map(([destDir, list]) => ({ destDir, paths: list })),
    );
  };

  const count = (plans ?? []).filter((p) => checked.has(p.path) && destOf(p) !== null).length;
  const moved = overrides.size;

  return (
    <Dialog
      title="자동 분류"
      onClose={onClose}
      className="dialog--classify"
      actions={
        <>
          {moved > 0 && (
            <button type="button" className="btn btn--ghost classify__reset" onClick={() => setOverrides(new Map())}>
              <RotateCcw size={15} aria-hidden="true" />
              옮긴 것 모두 되돌리기({moved})
            </button>
          )}
          <span className="classify__spacer" />
          <button type="button" className="btn btn--ghost" onClick={onClose}>
            취소
          </button>
          <button type="button" className="btn btn--primary" onClick={run} disabled={count === 0}>
            {count}개 나누기
          </button>
        </>
      }
    >
      <p className="dialog__sub">
        파일 이름과 형식을 보고 이렇게 나눌게요. <strong>다른 곳에 넣고 싶은 파일은 오른쪽 폴더로 끌어다 놓으세요.</strong>{" "}
        빼고 싶은 파일은 체크를 끄세요. 없는 폴더는 새로 만들고, 나눈 뒤에도 되돌릴 수 있어요.
      </p>
      {error && <p className="error-text">{error}</p>}
      {!plans && !error && <p className="dialog__sub">살펴보는 중…</p>}
      {plans && (
        <DndContext sensors={sensors} onDragStart={onDragStart} onDragEnd={onDragEnd} onDragCancel={() => setDragging(null)}>
          <div className="classify">
            <div className="classify__list" aria-label="나눌 파일">
              {groups.map(([dest, list]) => (
                <Group key={dest} dest={dest} title={displayPath(root, dest, separator)} isNew={list.every((p) => !overrides.has(p.path) && p.target && !p.target.exists)} count={list.length}>
                  {list.map((p) => (
                    <FileRow
                      key={p.path}
                      plan={p}
                      checked={checked.has(p.path)}
                      moved={overrides.has(p.path)}
                      why={overrides.has(p.path) ? "직접 옮김" : p.target ? reasonLabel(p.target.reason) : ""}
                      onToggle={() => toggle(p.path)}
                      onReset={() => moveTo(p, p.target?.destDir ?? null)}
                    />
                  ))}
                </Group>
              ))}
              <Group dest={null} title={`정리함에 남는 파일 ${left.length}개`} count={null}>
                {left.length === 0 ? (
                  <p className="classify__note">여기에 끌어다 놓으면 이번에는 나누지 않고 정리함에 남겨요.</p>
                ) : (
                  <>
                    <p className="classify__note">이름과 형식으로 넣을 곳을 정하지 못했거나 남기기로 한 파일이에요. 오른쪽 폴더로 끌어다 놓으면 거기에 넣어요.</p>
                    {left.map((p) => (
                      <FileRow
                        key={p.path}
                        plan={p}
                        checked={false}
                        moved={overrides.has(p.path)}
                        why={overrides.has(p.path) ? "남기기로 함" : ""}
                        onReset={() => moveTo(p, p.target?.destDir ?? null)}
                      />
                    ))}
                  </>
                )}
              </Group>
              {plans.length === 0 && <p className="dialog__sub">나눌 파일이 없어요.</p>}
            </div>
            <div className="classify__side">
              <div className="classify__side-title">끌어다 놓을 폴더</div>
              <FolderPane
                root={root}
                separator={separator}
                dragging={dragging !== null}
                toast={null}
                onToast={() => {}}
                label="끌어다 놓을 폴더"
                fill
              />
            </div>
          </div>
          <DragOverlay dropAnimation={null}>
            {dragging && (
              <div className="classify__drag">
                <FileBadge name={dragging.name} size={24} />
                <span>{dragging.name}</span>
              </div>
            )}
          </DragOverlay>
        </DndContext>
      )}
    </Dialog>
  );
}

/** 넣을 폴더 하나의 묶음. 파일을 끌어 이 묶음 위에 놓아도 이 폴더로 바뀐다. */
function Group({
  dest,
  title,
  isNew,
  count,
  children,
}: {
  dest: string | null;
  title: string;
  isNew?: boolean;
  count: number | null;
  children: React.ReactNode;
}) {
  const { setNodeRef, isOver } = useDroppable({
    id: dest === null ? "classify:inbox" : `classify:${dest}`,
    data: (dest === null ? { inbox: true } : { path: dest }) satisfies Drop,
  });
  return (
    <section ref={setNodeRef} className={`classify__group${dest === null ? " classify__group--left" : ""}${isOver ? " classify__group--over" : ""}`}>
      <div className="classify__head">
        {dest !== null && <FolderIcon size={20} />}
        <span className="classify__path" title={dest ?? undefined}>
          {title}
        </span>
        {isNew && <span className="classify__new">새 폴더</span>}
        {count !== null && <span className="classify__count">{count}개</span>}
      </div>
      <ul className="classify__files">{children}</ul>
    </section>
  );
}

function FileRow({
  plan,
  checked,
  moved,
  why,
  onToggle,
  onReset,
}: {
  plan: ClassifyPlan;
  checked: boolean;
  moved: boolean;
  why: string;
  onToggle?: () => void;
  onReset: () => void;
}) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: `classify-file:${plan.path}`, data: { plan } });
  return (
    <li ref={setNodeRef} className={`classify__file${isDragging ? " classify__file--drag" : ""}${onToggle ? "" : " classify__file--off"}`}>
      <span className="classify__grip" {...listeners} {...attributes} aria-label={`${plan.name} 끌어서 옮기기`}>
        <GripVertical size={15} aria-hidden="true" />
      </span>
      {onToggle && <input type="checkbox" aria-label={`${plan.name} 나누기`} checked={checked} onChange={onToggle} />}
      <span className="classify__name" title={plan.name} {...listeners}>
        {plan.name}
      </span>
      {why && <span className={`classify__why${moved ? " classify__why--moved" : ""}`}>{why}</span>}
      {moved && (
        <button type="button" className="classify__undo" onClick={onReset} title="원래 자동 분류대로" aria-label={`${plan.name} 원래대로`}>
          <RotateCcw size={14} aria-hidden="true" />
        </button>
      )}
    </li>
  );
}
