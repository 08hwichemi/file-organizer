import { useEffect, useMemo, useState } from "react";
import {
  DndContext,
  DragOverlay,
  pointerWithin,
  PointerSensor,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import { followCursor } from "../lib/dnd";
import { FolderPlus, GripVertical, Inbox, LayoutGrid, List, RotateCcw } from "lucide-react";
import { classifyPreview, errorMessage, folderClassifyPreview, listFiles, listFolders } from "../lib/backend";
import { baseName, displayPath } from "../lib/paths";
import { reasonLabel } from "../lib/rules";
import type { ClassifyPlan, FileEntry } from "../lib/types";
import Band from "./Band";
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
  /**
   * 기존 폴더 다시 정리하기: 정리함 파일 대신 이 폴더의 파일을 읽어 나눈다(SPEC 3-2).
   * 모르는 파일은 원래 자리에 그대로 두고, 같은 낱말이 많은 파일은 새 폴더 카드로 제안한다.
   */
  source?: { dir: string; title: string };
  root: string;
  separator: string;
  onClose: () => void;
  /** auto = 자동 분류대로 넣을 파일, manual = 직접 옮긴 파일(폴더별) */
  onRun: (auto: string[], manual: { destDir: string; paths: string[] }[]) => void;
}

/**
 * 자동 분류 미리보기(SPEC 4-1). 넣을 폴더마다 네모 카드(포스트잇을 줄 맞춰 붙인 것처럼)에
 * 그 폴더로 갈 파일을 담아 보여 준다. 카드는 모두 같은 크기이고, 파일이 많으면 카드 안에서 내려 본다.
 * 파일을 누르면 골라지고(다시 누르면 풀림), 고른 파일을 끌어 다른 카드에 놓으면 고른 파일이 다 같이 옮겨진다.
 * 폴더 카드에 있는 파일은 모두 나누고, 왼쪽 칸(정리함에 남기기 · 그대로 두기)의 파일만 옮기지 않는다(사용자 요청: 체크 상자 없이 하나로).
 * [+ 폴더 추가]로 목록에 없는 폴더 카드를 만든다.
 * 정리 폴더 안의 폴더는 추천된 파일이 없어도 늘 카드로 보인다(빈 카드는 반 높이).
 * 위의 버튼으로 '목록으로 보기'(왼쪽 목록 + 오른쪽 폴더 격자, 아무 폴더에나 놓기)로 바꿀 수 있다.
 */
export default function ClassifyDialog({ paths, source, root, separator, onClose, onRun }: Props) {
  const [plans, setPlans] = useState<ClassifyPlan[] | null>(null);
  /** 다시 정리하기: 안쪽 폴더의 파일도 읽을지, 파일이 많아 일부만 읽었는지 */
  const [includeSub, setIncludeSub] = useState(false);
  const [truncated, setTruncated] = useState(false);
  /** 고른 파일(파란색). 끌면 다 같이 옮겨진다 */
  const [selected, setSelected] = useState<Set<string>>(new Set());
  /** 직접 옮긴 파일 → 넣을 폴더(INBOX = 정리함에 남기기) */
  const [overrides, setOverrides] = useState<Map<string, string>>(new Map());
  /** 카드 순서(끌어 옮겨도 카드 자리는 그대로): 파일이 추천된 폴더 → (정리함에 남기기) → 정리 폴더 안의 나머지 폴더 */
  const [cards, setCards] = useState<string[]>([]);
  /** 앞에서 몇 장이 파일이 추천된 폴더인지(그 뒤에 '정리함에 남기기' 카드) */
  const [split, setSplit] = useState(0);
  /** 추천은 없었지만 정리 폴더 안에 있어서 카드로 보여 주는 폴더 */
  const [folderCards, setFolderCards] = useState<Set<string>>(new Set());
  /** 카드 폴더에 이미 들어 있는 파일(무엇이 있는지 보고 나눌 수 있게). 아직 없는 폴더는 빈 목록. */
  const [existing, setExisting] = useState<Map<string, FileEntry[]>>(new Map());
  /** 끄는 중인 파일: 잡은 파일(lead)과 같이 옮겨질 파일들 */
  const [dragging, setDragging] = useState<{ lead: ClassifyPlan; plans: ClassifyPlan[] } | null>(null);
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
  /** 새로 생긴 카드(폴더 추가, 목록 보기의 폴더 격자에 놓았을 때): 아래쪽에 생겨도 보이게 그 카드까지 내려 준다 */
  const [reveal, setReveal] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 5 } }));

  useEffect(() => {
    let alive = true;
    setPlans(null);
    setOverrides(new Map());
    setSelected(new Set());
    setError(null);
    const load = source
      ? folderClassifyPreview(source.dir, includeSub).then((r) => {
          setTruncated(r.truncated);
          return r.plans;
        })
      : classifyPreview(paths);
    // 폴더 목록을 못 읽어도 자동 분류는 보여 준다
    Promise.all([load, listFolders(root).catch(() => [])])
      .then(([p, folders]) => {
        if (!alive) return;
        setPlans(p);
        // 처음 카드 순서: 파일이 많은 폴더부터, 그다음 정리 폴더 안의 나머지 폴더(파일이 없어도 늘 보이게)
        const count = new Map<string, number>();
        for (const x of p) if (x.target) count.set(x.target.destDir, (count.get(x.target.destDir) ?? 0) + 1);
        const suggested = [...count.entries()].sort((a, b) => b[1] - a[1]).map(([d]) => d);
        const rest = folders.map((f) => f.path).filter((d) => !count.has(d));
        setCards([...suggested, ...rest]);
        setSplit(suggested.length);
        setFolderCards(new Set(rest));
      })
      .catch((e) => alive && setError(errorMessage(e)));
    return () => {
      alive = false;
    };
  }, [paths, root, source, includeSub]);

  useEffect(() => {
    if (!reveal) return;
    const card = [...document.querySelectorAll<HTMLElement>(".classify__card[data-dest]")].find((el) => el.dataset.dest === reveal);
    card?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    setReveal(null);
  }, [reveal, cards]);

  // 카드가 생기면 그 폴더에 이미 있는 파일을 읽는다(폴더 하나씩, 이미 읽은 것은 다시 읽지 않음)
  useEffect(() => {
    let alive = true;
    for (const dest of cards) {
      if (existing.has(dest)) continue;
      setExisting((m) => new Map(m).set(dest, []));
      listFiles(dest)
        .then((files) => alive && setExisting((m) => new Map(m).set(dest, files)))
        .catch(() => {
          // 아직 없는 폴더(새 폴더) 등은 빈 목록 그대로
        });
    }
    return () => {
      alive = false;
    };
    // existing은 일부러 빼 둔다(읽은 결과가 들어올 때마다 다시 돌지 않게)
  }, [cards]);

  const destOf = (p: ClassifyPlan) => overrides.get(p.path) ?? p.target?.destDir ?? INBOX;

  const byCard = useMemo(() => {
    const map = new Map<string, ClassifyPlan[]>();
    for (const p of plans ?? []) {
      const d = overrides.get(p.path) ?? p.target?.destDir ?? INBOX;
      map.set(d, [...(map.get(d) ?? []), p]);
    }
    return map;
  }, [plans, overrides]);

  /** 파일 하나 고르기·풀기 */
  const toggleSelect = (path: string) =>
    setSelected((c) => {
      const next = new Set(c);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  /** 카드의 파일을 모두 고르거나(on) 고른 것을 푼다 */
  const selectMany = (list: string[], on: boolean) =>
    setSelected((c) => {
      const next = new Set(c);
      for (const p of list) {
        if (on) next.add(p);
        else next.delete(p);
      }
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
    if (dest !== INBOX) {
      if (!cards.includes(dest)) setReveal(dest);
      addCard(dest);
    }
  };

  /** 고른 파일 */
  const selectedPlans = (plans ?? []).filter((p) => selected.has(p.path));

  // 고른 파일을 잡으면 고른 파일이 다 같이, 고르지 않은 파일을 잡으면 그 파일만
  const onDragStart = (e: DragStartEvent) => {
    const lead = (e.active.data.current as { plan: ClassifyPlan }).plan;
    setDragging({ lead, plans: selected.has(lead.path) ? selectedPlans : [lead] });
  };
  const onDragEnd = (e: DragEndEvent) => {
    const moving = dragging?.plans ?? [];
    setDragging(null);
    // 카드·묶음은 { dest }, 목록 보기의 폴더 격자는 { path }
    const data = e.over?.data.current as { dest?: string; path?: string } | undefined;
    const dest = data?.dest ?? data?.path;
    if (!dest) return;
    for (const p of moving) if (dest !== destOf(p)) moveTo(p, dest);
    setSelected(new Set());
  };

  /** 고른 파일을 왼쪽 칸(정리함에 남기기 · 그대로 두기)으로 */
  const leaveSelected = () => {
    for (const p of selectedPlans) if (destOf(p) !== INBOX) moveTo(p, INBOX);
    setSelected(new Set());
  };

  const run = () => {
    const auto: string[] = [];
    const manual = new Map<string, string[]>();
    for (const p of plans ?? []) {
      const o = overrides.get(p.path);
      if (o === INBOX) continue;
      // 다시 정리하기는 미리보기에서 정한 곳(새 폴더 제안 포함)으로 그대로 넣는다
      const dest = o ?? (source ? p.target?.destDir : undefined);
      if (dest) manual.set(dest, [...(manual.get(dest) ?? []), p.path]);
      else if (p.target) auto.push(p.path);
    }
    onRun(
      auto,
      [...manual.entries()].map(([destDir, list]) => ({ destDir, paths: list })),
    );
  };

  /** 나눌 파일 = 폴더 카드에 있는 파일 */
  const count = (plans ?? []).filter((p) => destOf(p) !== INBOX).length;
  const moved = overrides.size;
  const leftName = source ? "그대로 두기" : "정리함에 남기기";
  /** 카드의 파일을 다 골랐는지 */
  const allSelected = (list: ClassifyPlan[]) => list.length > 0 && list.every((p) => selected.has(p.path));
  const left = byCard.get(INBOX) ?? [];

  /** 넣을 폴더 카드 하나. 파일이 없는 카드는 카드 보기에서 반 높이 */
  const folderCard = (dest: string) => {
    const list = byCard.get(dest) ?? [];
    return (
      <Card
        key={dest}
        dest={dest}
        name={cardName(root, dest, separator)}
        path={displayPath(root, dest, separator)}
        isNew={list.length > 0 && list.every((p) => !overrides.has(p.path) && p.target && !p.target.exists)}
        count={list.length}
        allSelected={allSelected(list)}
        onAll={(on) => selectMany(list.map((p) => p.path), on)}
        existing={existing.get(dest) ?? []}
      >
        {list.length === 0 && <li className="classify__empty">여기로 끌어다 놓으세요</li>}
        {list.map((p) => (
          <FileRow
            key={p.path}
            plan={p}
            selected={selected.has(p.path)}
            moved={overrides.has(p.path)}
            why={overrides.has(p.path) ? "직접 옮김" : p.target ? reasonLabel(p.target.reason) : ""}
            onSelect={() => toggleSelect(p.path)}
            onReset={() => moveTo(p, p.target?.destDir ?? INBOX)}
          />
        ))}
      </Card>
    );
  };

  const inboxCard = () => (
    <Card
      dest={INBOX}
      name={leftName}
      path={source ? "원래 폴더에 남아요" : "이번에는 나누지 않아요"}
      count={left.length}
      allSelected={allSelected(left)}
      onAll={(on) => selectMany(left.map((p) => p.path), on)}
      inbox
    >
      {left.length === 0 && (
        <li className="classify__empty">
          {source ? "여기로 끌어다 놓으면 옮기지 않아요" : "여기로 끌어다 놓으면 정리함에 남아요"}
        </li>
      )}
      {left.map((p) => (
        <FileRow
          key={p.path}
          plan={p}
          selected={selected.has(p.path)}
          moved={overrides.has(p.path)}
          why={
            overrides.has(p.path) && p.target
              ? `${cardName(root, p.target.destDir, separator)}에서 뺌`
              : "구별 못 함"
          }
          onSelect={() => toggleSelect(p.path)}
          onReset={() => moveTo(p, p.target?.destDir ?? INBOX)}
        />
      ))}
    </Card>
  );

  /**
   * 카드 보기: 정리함에 남기기는 왼쪽에 따로 길게 두고, 폴더 카드는 추천된 폴더 → 정리 폴더 안의 나머지 폴더(늘 보임).
   * 목록 보기: 추천된 폴더 → 정리함에 남기기 → 파일이 든 나머지 폴더(오른쪽에 폴더 격자가 따로 있으므로 빈 묶음은 뺀다).
   */
  const groupCards = (view: View) => {
    const all = view === "cards";
    const show = (d: string) => all || !folderCards.has(d) || (byCard.get(d)?.length ?? 0) > 0;
    return (
      <>
        {cards.slice(0, split).map(folderCard)}
        {!all && inboxCard()}
        {cards.slice(split).filter(show).map(folderCard)}
      </>
    );
  };

  return (
    <Dialog
      title={source ? `${source.title} · ${baseName(source.dir)}` : "자동 분류"}
      onClose={onClose}
      className="dialog--classify"
      actions={
        <>
          {selected.size > 0 ? (
            // 고른 파일이 있으면: n개 고름 · [정리함에 남기기로] [고르기 풀기]
            <div className="classify__selbar">
              <span className="classify__selbar-text">{selected.size}개 고름</span>
              {selectedPlans.some((p) => destOf(p) !== INBOX) && (
                <button type="button" className="btn" onClick={leaveSelected}>
                  {leftName}로
                </button>
              )}
              <button type="button" className="btn btn--ghost" onClick={() => setSelected(new Set())}>
                고르기 풀기
              </button>
            </div>
          ) : (
            moved > 0 && (
              <button type="button" className="btn btn--ghost classify__reset" onClick={() => setOverrides(new Map())}>
                <RotateCcw size={15} aria-hidden="true" />
                옮긴 것 모두 되돌리기({moved})
              </button>
            )
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
        {source ? (
          <p className="dialog__sub">
            <strong>{displayPath(root, source.dir, separator)}</strong>
            {plans ? `의 파일 ${plans.length}개를` : "의 파일을"} 폴더마다 모아 보여 드려요.{" "}
            <strong>파일을 누르면 골라지고, 고른 파일을 끌면 한꺼번에 옮겨져요.</strong> 옮기지 않을 파일은 왼쪽
            ‘그대로 두기’로 옮기세요. 모르는 파일은 원래 자리에 그대로 둬요.
            {truncated && " 파일이 많아 앞의 2,000개만 보여요."}
          </p>
        ) : (
          <p className="dialog__sub">
            폴더마다 들어갈 파일을 모아 보여 드려요. <strong>파일을 누르면 골라지고, 고른 파일을 끌면 한꺼번에 옮겨져요.</strong>{" "}
            넣지 않을 파일은 왼쪽 ‘정리함에 남기기’로 옮기세요. 없는 폴더는 새로 만들고, 나눈 뒤에도 되돌릴 수 있어요.
          </p>
        )}
        <div className="classify__opts">
          {source && (
            <label className="classify__sub">
              <input type="checkbox" checked={includeSub} onChange={(e) => setIncludeSub(e.target.checked)} />
              안쪽 폴더의 파일도
            </label>
          )}
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
      </div>
      {error && <p className="error-text">{error}</p>}
      {!plans && !error && <p className="dialog__sub">살펴보는 중…</p>}
      {plans && plans.length === 0 && (
        <p className="dialog__sub">
          {source && !includeSub ? "이 폴더 바로 안에는 나눌 파일이 없어요. ‘안쪽 폴더의 파일도’를 켜 보세요." : "나눌 파일이 없어요."}
        </p>
      )}
      {plans && plans.length > 0 && (
        <DndContext sensors={sensors} collisionDetection={pointerWithin} onDragStart={onDragStart} onDragEnd={onDragEnd} onDragCancel={() => setDragging(null)}>
          {view === "cards" ? (
            // 왼쪽: 정리함에 남기기(창 높이만큼 길게, 그 안에서 내려 봄) / 오른쪽: 폴더 카드(오른쪽만 내려 봄)
            <div className="classify-split">
              {inboxCard()}
              <div className="classify__board">
                {groupCards("cards")}
                <button type="button" className="classify__card classify__card--add" onClick={() => setAdding(true)}>
                  <FolderPlus size={26} aria-hidden="true" />
                  <span>폴더 추가</span>
                  <small>안쪽 폴더나 목록에 없는 폴더로 보낼 때</small>
                </button>
              </div>
            </div>
          ) : (
            <div className="classify">
              <div className="classify__list" aria-label="나눌 파일">
                {groupCards("list")}
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
          <DragOverlay dropAnimation={null} modifiers={[followCursor]}>
            {dragging && (
              <div className="classify__drag">
                <FileBadge name={dragging.lead.name} size={24} />
                <span>{dragging.lead.name}</span>
                {dragging.plans.length > 1 && <b className="classify__drag-more">외 {dragging.plans.length - 1}</b>}
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
                  if (pick) {
                    if (!cards.includes(pick)) setReveal(pick);
                    addCard(pick);
                  }
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

/**
 * 넣을 폴더 하나 = 네모 카드. 파일을 끌어 이 카드에 놓으면 이 폴더로 바뀐다.
 * 카드 보기에서는 모든 카드가 같은 크기(줄 맞춰 깔끔하게). 파란 띠 "넣을 파일 n" 아래에 넣을 파일,
 * 회색 띠 "이미 있는 파일 n" 아래에 폴더에 원래 있는 파일(회색 바탕)을 보여 줘서 무엇이 들어 있는 폴더인지
 * 보고 나눌 수 있다. 칸마다 따로 내려 보고, 넣을 파일 칸은 3줄까지(반 줄이 걸치지 않게).
 */
function Card({
  dest,
  name,
  path,
  isNew,
  count,
  allSelected,
  onAll,
  inbox,
  existing = [],
  children,
}: {
  dest: string;
  name: string;
  path: string;
  isNew?: boolean;
  count: number;
  /** 이 카드의 파일을 다 골랐는지 */
  allSelected?: boolean;
  /** 이 카드의 파일을 모두 고르거나(true) 고른 것을 푼다(false) */
  onAll?: (on: boolean) => void;
  inbox?: boolean;
  /** 이 폴더에 이미 있는 파일 */
  existing?: FileEntry[];
  children: React.ReactNode;
}) {
  const { setNodeRef, isOver } = useDroppable({ id: `classify-card:${dest}`, data: { dest } });
  return (
    <section
      ref={setNodeRef}
      data-dest={dest}
      className={`classify__card${inbox ? " classify__card--inbox" : ""}${existing.length > 0 ? " classify__card--has" : ""}${isOver ? " classify__card--over" : ""}`}
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
        {/* 폴더 카드는 아래 "넣을 파일 n" 띠에 수가 있어 머리에는 두지 않는다 */}
        {inbox && <span className="classify__count">{count}</span>}
      </div>
      {(!inbox || count > 0) && (
        <Band
          tone={inbox ? "gray" : "blue"}
          title={inbox ? "남길 파일" : "넣을 파일"}
          count={count}
          hint={
            count > 0 &&
            onAll && (
              <button type="button" className="classify__all" onClick={() => onAll(!allSelected)}>
                {allSelected ? "고르기 풀기" : "모두 고르기"}
              </button>
            )
          }
        />
      )}
      <ul className="classify__files">{children}</ul>
      {existing.length > 0 && (
        <div className="classify__have">
          <Band tone="gray" title="이미 있는 파일" count={existing.length} />
          <ul className="classify__have-list" aria-label={`${name}에 이미 있는 파일`}>
            {existing.map((f) => (
              <li key={f.path} title={f.name}>
                {f.name}
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}

/** 파일 한 줄: 누르면 고르기·풀기(파란색), 잡고 끌면 옮기기(고른 파일이면 고른 파일 모두) */
function FileRow({
  plan,
  selected,
  moved,
  why,
  onSelect,
  onReset,
}: {
  plan: ClassifyPlan;
  selected: boolean;
  moved: boolean;
  why: string;
  onSelect: () => void;
  onReset: () => void;
}) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({ id: `classify-file:${plan.path}`, data: { plan } });
  return (
    <li
      ref={setNodeRef}
      className={`classify__file${selected ? " classify__file--sel" : ""}${isDragging ? " classify__file--drag" : ""}`}
      title={why ? `${plan.name} · ${why}` : plan.name}
      {...listeners}
      {...attributes}
      role="button"
      aria-pressed={selected}
      aria-label={`${plan.name} ${selected ? "고름" : "고르기"}`}
      onClick={onSelect}
      onKeyDown={(e) => {
        if (e.key === " " || e.key === "Enter") {
          e.preventDefault();
          onSelect();
        }
      }}
    >
      <span className="classify__grip" aria-hidden="true">
        <GripVertical size={14} />
      </span>
      <span className="classify__name">{plan.name}</span>
      {why && <span className={`classify__why${moved ? " classify__why--moved" : ""}`}>{why}</span>}
      {moved && (
        <button
          type="button"
          className="classify__undo"
          onClick={(e) => {
            e.stopPropagation();
            onReset();
          }}
          onPointerDown={(e) => e.stopPropagation()}
          onKeyDown={(e) => e.stopPropagation()}
          title="원래 자동 분류대로"
          aria-label={`${plan.name} 원래대로`}
        >
          <RotateCcw size={13} aria-hidden="true" />
        </button>
      )}
    </li>
  );
}
