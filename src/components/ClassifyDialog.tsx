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
import { FolderPlus, GripVertical, Inbox, LayoutGrid, List, RotateCcw } from "lucide-react";
import { classifyPreview, errorMessage } from "../lib/backend";
import { displayPath } from "../lib/paths";
import { reasonLabel } from "../lib/rules";
import type { ClassifyPlan } from "../lib/types";
import Dialog from "./Dialog";
import FileBadge from "./FileBadge";
import FolderIcon from "./FolderIcon";
import FolderPane from "./FolderPane";
import FolderPicker from "./FolderPicker";
import "./ClassifyDialog.css";

/** 카드 이름: 마지막 폴더 이름. 연도·학기·학급 폴더(2027, 2학기, 3-2)는 위 폴더도 붙여 "수행평가 › 2학기" */
function cardName(root: string, dest: string, separator: string): string {
  const parts = displayPath(root, dest, separator).split(" › ");
  const vague = (n: string) => /^(20\d\d|\d{2}학년도|20\d\d학년도|[12]학기|20\d\d [12]학기|\d-\d{1,2}|\d반)$/.test(n);
  // 끝에서부터 연도·학기·학급 폴더를 지나 처음 나오는 뜻 있는 폴더까지(정리 폴더 이름은 빼고)
  let i = parts.length - 1;
  while (i > 1 && vague(parts[i])) i -= 1;
  return parts.slice(i).join(" › ");
}

/** 정리함에 남기기 카드의 열쇠 */
const INBOX = "\u0000inbox";

/** 보는 방식: 카드(폴더마다 네모 카드) / 목록(왼쪽 목록 + 오른쪽 폴더 격자). 고른 것을 기억한다. */
type View = "cards" | "list";
const VIEW_KEY = "classify-view";

function savedView(): View {
  try {
    return window.localStorage.getItem(VIEW_KEY) === "list" ? "list" : "cards";
  } catch {
    return "cards";
  }
}

interface Props {
  /** 나눌 파일(정리함에서 지금 보이는 파일) */
  paths: string[];
  root: string;
  separator: string;
  onClose: () => void;
  /** auto = 자동 분류대로 넣을 파일, manual = 직접 옮긴 파일(폴더별) */
  onRun: (auto: string[], manual: { destDir: string; paths: string[] }[]) => void;
}

/**
 * 자동 분류 미리보기(SPEC 4-1). 넣을 폴더마다 같은 크기의 네모 카드(포스트잇을 줄 맞춰 붙인 것처럼)에
 * 그 폴더로 갈 파일을 담아 보여 준다. 파일이 많으면 카드 안에서만 내려간다.
 * 파일을 끌어 다른 카드에 놓으면 그 파일만 넣을 곳이 바뀐다. [+ 폴더 추가]로 목록에 없는 폴더 카드를 만든다.
 * 위의 버튼으로 '목록으로 보기'(왼쪽 목록 + 오른쪽 폴더 격자, 아무 폴더에나 놓기)로 바꿀 수 있다.
 */
export default function ClassifyDialog({ paths, root, separator, onClose, onRun }: Props) {
  const [plans, setPlans] = useState<ClassifyPlan[] | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  /** 직접 옮긴 파일 → 넣을 폴더(INBOX = 정리함에 남기기) */
  const [overrides, setOverrides] = useState<Map<string, string>>(new Map());
  /** 카드 순서(끌어 옮겨도 카드 자리는 그대로) */
  const [cards, setCards] = useState<string[]>([]);
  const [dragging, setDragging] = useState<ClassifyPlan | null>(null);
  const [adding, setAdding] = useState(false);
  const [view, setViewState] = useState<View>(savedView);
  const setView = (v: View) => {
    setViewState(v);
    try {
      window.localStorage.setItem(VIEW_KEY, v);
    } catch {
      // 기억하지 못해도 보기는 바뀐다
    }
  };
  const [pick, setPick] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 5 } }));

  useEffect(() => {
    let alive = true;
    classifyPreview(paths)
      .then((p) => {
        if (!alive) return;
        setPlans(p);
        setChecked(new Set(p.filter((x) => x.target).map((x) => x.path)));
        // 처음 카드 순서: 파일이 많은 폴더부터
        const count = new Map<string, number>();
        for (const x of p) if (x.target) count.set(x.target.destDir, (count.get(x.target.destDir) ?? 0) + 1);
        setCards([...count.entries()].sort((a, b) => b[1] - a[1]).map(([d]) => d));
      })
      .catch((e) => alive && setError(errorMessage(e)));
    return () => {
      alive = false;
    };
  }, [paths]);

  const destOf = (p: ClassifyPlan) => overrides.get(p.path) ?? p.target?.destDir ?? INBOX;

  const byCard = useMemo(() => {
    const map = new Map<string, ClassifyPlan[]>();
    for (const p of plans ?? []) {
      const d = overrides.get(p.path) ?? p.target?.destDir ?? INBOX;
      map.set(d, [...(map.get(d) ?? []), p]);
    }
    return map;
  }, [plans, overrides]);

  const toggle = (path: string) =>
    setChecked((c) => {
      const next = new Set(c);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const addCard = (dest: string) => setCards((c) => (c.includes(dest) ? c : [...c, dest]));

  const moveTo = (plan: ClassifyPlan, dest: string) => {
    const original = plan.target?.destDir ?? INBOX;
    setOverrides((o) => {
      const next = new Map(o);
      if (dest === original) next.delete(plan.path);
      else next.set(plan.path, dest);
      return next;
    });
    setChecked((c) => {
      const next = new Set(c);
      if (dest === INBOX) next.delete(plan.path);
      else next.add(plan.path);
      return next;
    });
    if (dest !== INBOX) addCard(dest);
  };

  const onDragStart = (e: DragStartEvent) => setDragging((e.active.data.current as { plan: ClassifyPlan }).plan);
  const onDragEnd = (e: DragEndEvent) => {
    setDragging(null);
    const plan = (e.active.data.current as { plan: ClassifyPlan } | undefined)?.plan;
    // 카드·묶음은 { dest }, 목록 보기의 폴더 격자는 { path }
    const data = e.over?.data.current as { dest?: string; path?: string } | undefined;
    const dest = data?.dest ?? data?.path;
    if (plan && dest && dest !== destOf(plan)) moveTo(plan, dest);
  };

  const run = () => {
    const auto: string[] = [];
    const manual = new Map<string, string[]>();
    for (const p of plans ?? []) {
      if (!checked.has(p.path)) continue;
      const o = overrides.get(p.path);
      if (o === INBOX) continue;
      if (o) manual.set(o, [...(manual.get(o) ?? []), p.path]);
      else if (p.target) auto.push(p.path);
    }
    onRun(
      auto,
      [...manual.entries()].map(([destDir, list]) => ({ destDir, paths: list })),
    );
  };

  const count = (plans ?? []).filter((p) => checked.has(p.path) && destOf(p) !== INBOX).length;
  const moved = overrides.size;
  const left = byCard.get(INBOX) ?? [];

  /** 넣을 폴더마다 카드(목록 보기에서는 위아래로 쌓인 묶음) + 정리함에 남기기 */
  const groupCards = () => (
    <>
      {cards.map((dest) => {
        const list = byCard.get(dest) ?? [];
        return (
          <Card
            key={dest}
            dest={dest}
            name={cardName(root, dest, separator)}
            path={displayPath(root, dest, separator)}
            isNew={list.length > 0 && list.every((p) => !overrides.has(p.path) && p.target && !p.target.exists)}
            count={list.length}
          >
            {list.length === 0 && <li className="classify__empty">여기로 끌어다 놓으세요</li>}
            {list.map((p) => (
              <FileRow
                key={p.path}
                plan={p}
                checked={checked.has(p.path)}
                moved={overrides.has(p.path)}
                why={overrides.has(p.path) ? "직접 옮김" : p.target ? reasonLabel(p.target.reason) : ""}
                onToggle={() => toggle(p.path)}
                onReset={() => moveTo(p, p.target?.destDir ?? INBOX)}
              />
            ))}
          </Card>
        );
      })}
      <Card dest={INBOX} name="정리함에 남기기" path="이번에는 나누지 않아요" count={left.length} inbox>
        {left.length === 0 && <li className="classify__empty">여기로 끌어다 놓으면 정리함에 남아요</li>}
        {left.map((p) => (
          <FileRow
            key={p.path}
            plan={p}
            moved={overrides.has(p.path)}
            why={overrides.has(p.path) ? "남기기로 함" : "구별 못 함"}
            onReset={() => moveTo(p, p.target?.destDir ?? INBOX)}
          />
        ))}
      </Card>
    </>
  );

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
      <div className="classify__top">
        <p className="dialog__sub">
          폴더마다 들어갈 파일을 모아 보여 드려요. <strong>다른 폴더에 넣고 싶은 파일은 그 폴더로 끌어다 놓으세요.</strong>{" "}
          빼고 싶은 파일은 체크를 끄세요. 없는 폴더는 새로 만들고, 나눈 뒤에도 되돌릴 수 있어요.
        </p>
        <div className="classify__views" role="group" aria-label="보는 방식">
          <button type="button" aria-pressed={view === "cards"} onClick={() => setView("cards")}>
            <LayoutGrid size={15} aria-hidden="true" />
            카드로 보기
          </button>
          <button type="button" aria-pressed={view === "list"} onClick={() => setView("list")}>
            <List size={15} aria-hidden="true" />
            목록으로 보기
          </button>
        </div>
      </div>
      {error && <p className="error-text">{error}</p>}
      {!plans && !error && <p className="dialog__sub">살펴보는 중…</p>}
      {plans && plans.length === 0 && <p className="dialog__sub">나눌 파일이 없어요.</p>}
      {plans && plans.length > 0 && (
        <DndContext sensors={sensors} onDragStart={onDragStart} onDragEnd={onDragEnd} onDragCancel={() => setDragging(null)}>
          {view === "cards" ? (
            <div className="classify__board">
              {groupCards()}
              <button type="button" className="classify__card classify__card--add" onClick={() => setAdding(true)}>
                <FolderPlus size={26} aria-hidden="true" />
                <span>폴더 추가</span>
                <small>목록에 없는 폴더로 보내고 싶을 때</small>
              </button>
            </div>
          ) : (
            <div className="classify">
              <div className="classify__list" aria-label="나눌 파일">
                {groupCards()}
              </div>
              <div className="classify__side">
                <div className="classify__side-title">끌어다 놓을 폴더 · 누르면 안으로 들어가요</div>
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
          )}
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

      {adding && (
        <div className="classify__picker" role="dialog" aria-label="폴더 추가">
          <div className="classify__picker-card">
            <div className="classify__picker-title">카드로 추가할 폴더를 고르세요</div>
            <FolderPicker root={root} separator={separator} selected={pick} onSelect={setPick} reveal={null} />
            <div className="button-row classify__picker-actions">
              <button type="button" className="btn btn--ghost" onClick={() => setAdding(false)}>
                취소
              </button>
              <button
                type="button"
                className="btn btn--primary"
                disabled={!pick}
                onClick={() => {
                  if (pick) addCard(pick);
                  setAdding(false);
                  setPick(null);
                }}
              >
                카드 추가
              </button>
            </div>
          </div>
        </div>
      )}
    </Dialog>
  );
}

/** 넣을 폴더 하나 = 같은 크기의 네모 카드. 파일을 끌어 이 카드에 놓으면 이 폴더로 바뀐다. */
function Card({
  dest,
  name,
  path,
  isNew,
  count,
  inbox,
  children,
}: {
  dest: string;
  name: string;
  path: string;
  isNew?: boolean;
  count: number;
  inbox?: boolean;
  children: React.ReactNode;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: `classify-card:${dest}`, data: { dest } });
  return (
    <section
      ref={setNodeRef}
      className={`classify__card${inbox ? " classify__card--inbox" : ""}${isOver ? " classify__card--over" : ""}`}
      aria-label={`${name} ${count}개`}
    >
      <div className="classify__card-head">
        {inbox ? <Inbox size={20} aria-hidden="true" className="classify__inbox-icon" /> : <FolderIcon size={22} />}
        <div className="classify__card-title">
          <span className="classify__card-name" title={name}>
            {name}
          </span>
          <span className="classify__card-path" title={path}>
            {path}
          </span>
        </div>
        {isNew && <span className="classify__new">새 폴더</span>}
        <span className="classify__count">{count}</span>
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
  checked?: boolean;
  moved: boolean;
  why: string;
  onToggle?: () => void;
  onReset: () => void;
}) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: `classify-file:${plan.path}`, data: { plan } });
  return (
    <li
      ref={setNodeRef}
      className={`classify__file${isDragging ? " classify__file--drag" : ""}${onToggle ? "" : " classify__file--off"}`}
      title={why ? `${plan.name} · ${why}` : plan.name}
    >
      <span className="classify__grip" {...listeners} {...attributes} aria-label={`${plan.name} 끌어서 옮기기`}>
        <GripVertical size={14} aria-hidden="true" />
      </span>
      {onToggle && <input type="checkbox" aria-label={`${plan.name} 나누기`} checked={checked} onChange={onToggle} />}
      <span className="classify__name" {...listeners}>
        {plan.name}
      </span>
      {why && <span className={`classify__why${moved ? " classify__why--moved" : ""}`}>{why}</span>}
      {moved && (
        <button type="button" className="classify__undo" onClick={onReset} title="원래 자동 분류대로" aria-label={`${plan.name} 원래대로`}>
          <RotateCcw size={13} aria-hidden="true" />
        </button>
      )}
    </li>
  );
}
