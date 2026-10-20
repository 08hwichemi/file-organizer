import { useDraggable } from "@dnd-kit/core";
import { Fragment, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { ArrowRight, ChevronDown, Download, History, Trash2, Wand2 } from "lucide-react";
import { formatAgo } from "../lib/format";
import { displayPath, friendlyName, friendlyPlace, isInsidePath, samePath, type KnownFolders } from "../lib/paths";
import type { InboxItem } from "../lib/types";
import Band from "./Band";
import FileBadge from "./FileBadge";
import "./InboxFileList.css";

interface Props {
  items: InboxItem[];
  watchFolders: string[];
  known: KnownFolders | null;
  tab: string;
  onTab: (tab: string) => void;
  selected: Set<string>;
  onToggle: (path: string) => void;
  /** 이 파일들을 더 고른다(Shift로 사이 고르기) */
  onAdd: (paths: string[]) => void;
  onSelectMany: (paths: string[]) => void;
  onClear: () => void;
  dragging: string[] | null;
  onImport: () => void;
  importing: boolean;
  /** 고른 파일을 휴지통으로 */
  onTrash: () => void;
  /** 추천 위치에 넣기(줄의 [넣기]) */
  onAccept: (paths: string[]) => void;
  /** [자동 분류]: 지금 보이는 파일을 이름·형식으로 나누기(미리보기 뒤) */
  onClassify: (paths: string[]) => void;
  root: string | null;
  separator: string;
  busy: boolean;
  /** '최근 정리' 탭을 고르면 목록 대신 보여 줄 것 */
  recent: ReactNode;
}

export const ALL_TAB = "all";
/** 지켜볼 폴더·문서·사진 등 어디에도 속하지 않는 곳(D: 드라이브 등) */
const OTHER_GROUP = "\u0000other";

/** 새 파일이 온 폴더를 묶는 탭: 그 폴더를 품은 지켜볼 폴더 → 문서·사진·동영상·음악 → 다른 곳 */
function sourceGroup(dir: string, watchFolders: string[], known: KnownFolders | null): string {
  const holds = (root: string) => samePath(root, dir) || isInsidePath(root, dir);
  const watched = watchFolders.find(holds);
  if (watched) return watched;
  const place = [known?.documents, known?.pictures, known?.videos, known?.music, known?.desktop, known?.downloads].find(
    (p): p is string => !!p && holds(p),
  );
  return place ?? OTHER_GROUP;
}
/** 정리함 파일이 아니라 최근 정리한 파일을 보는 탭 */
export const RECENT_TAB = "\u0000recent";

/**
 * 정리함 왼쪽 파일 칸(SPEC 3-2). 맨 위 [정리할 파일 | 최근 정리], 그 아래 폴더 탭(전체·다운로드·바탕화면…),
 * 배지·이름·출처와 시각. 자동 분류 창과 같은 고르기(사용자 요청): 누르면 골라지고(파란색) 다시 누르면 풀리며,
 * Shift를 누른 채 누르면 마지막으로 누른 파일부터 거기까지 모두 고른다. 줄을 끌어 폴더에 놓을 수 있다(고른 파일이면 고른 것 모두).
 */
export default function InboxFileList(props: Props) {
  const { items, watchFolders, known, tab, onTab, selected, onToggle, onSelectMany, onClear } = props;

  // 폴더 탭: 지켜볼 폴더(그 하위 폴더 포함), 그다음 내 PC 어디든으로 찾은 곳(문서·사진 등, 그 밖은 "다른 곳")
  const groupOf = (dir: string) => sourceGroup(dir, watchFolders, known);
  const groupKeys = [...watchFolders];
  for (const i of items) {
    const g = groupOf(i.sourceDir);
    if (!groupKeys.some((k) => samePath(k, g))) groupKeys.push(g);
  }
  const sources = groupKeys
    .map((key) => ({
      key,
      label: key === OTHER_GROUP ? "다른 곳" : friendlyName(key, known),
      count: items.filter((i) => samePath(groupOf(i.sourceDir), key)).length,
    }))
    // 파일이 없는 폴더 탭은 숨긴다(지금 고른 탭은 남김)
    .filter((t) => t.count > 0 || t.key === tab);
  const tabs = [{ key: ALL_TAB, label: "전체", count: items.length }, ...sources];
  const showRecent = tab === RECENT_TAB;
  const visible = showRecent
    ? []
    : tab === ALL_TAB
      ? items
      : items.filter((i) => samePath(groupOf(i.sourceDir), tab));
  // 추천 위치가 있는 파일([넣기]만 누르면 됨, 파란 띠) / 직접 골라야 하는 파일(끌어다 놓기, 회색 띠)
  const groups = [
    {
      key: "sug",
      tone: "blue" as const,
      title: "추천 위치가 있는 파일",
      hint: "[넣기]로 바로",
      items: visible.filter((i) => i.suggestion),
    },
    {
      key: "nosug",
      tone: "gray" as const,
      title: "직접 골라야 하는 파일",
      hint: "오른쪽 폴더로 끌어다 놓기",
      items: visible.filter((i) => !i.suggestion),
    },
  ];
  const allVisibleSelected = visible.length > 0 && visible.every((i) => selected.has(i.path));
  // Shift로 사이를 고를 때의 기준(마지막으로 그냥 누른 파일)
  const anchor = useRef<string | null>(null);
  const pick = (path: string, shift: boolean) => {
    const order = groups.flatMap((g) => g.items.map((i) => i.path));
    const from = anchor.current ? order.indexOf(anchor.current) : -1;
    const to = order.indexOf(path);
    if (shift && from >= 0 && to >= 0) {
      props.onAdd(order.slice(Math.min(from, to), Math.max(from, to) + 1));
      return;
    }
    anchor.current = path;
    onToggle(path);
  };
  // '최근 정리'에서 '정리할 파일'로 돌아올 때 보던 폴더 탭으로
  const lastSource = useRef(ALL_TAB);
  if (!showRecent) lastSource.current = tab;

  const importButton = (
    <button
      type="button"
      className="ifiles__import"
      onClick={props.onImport}
      disabled={props.importing}
      title="다운로드·바탕화면 등 지켜볼 폴더에 지금 있는 파일을 정리함으로 불러와요"
    >
      <Download size={14} aria-hidden="true" />
      {props.importing ? "가져오는 중…" : "지금 있는 파일 가져오기"}
    </button>
  );

  return (
    <section className="ifiles" aria-label="정리할 파일">
      <div className="ifiles__head">
        <div className="ifiles__seg" role="tablist" aria-label="보기">
          <button
            type="button"
            role="tab"
            aria-selected={!showRecent}
            className={`ifiles__seg-btn${showRecent ? "" : " ifiles__seg-btn--on"}`}
            onClick={() => showRecent && onTab(lastSource.current)}
          >
            정리할 파일 <b>{items.length}</b>
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={showRecent}
            className={`ifiles__seg-btn${showRecent ? " ifiles__seg-btn--on" : ""}`}
            onClick={() => onTab(RECENT_TAB)}
            title="최근에 넣거나 옮긴 파일과 어디로 갔는지 보고, 되돌릴 수 있어요"
          >
            <History size={14} aria-hidden="true" />
            최근 정리
          </button>
        </div>
      </div>

      {/* 폴더가 하나뿐이면 '전체'와 같으므로 탭 줄을 두지 않는다 */}
      {!showRecent && (sources.length > 1 || tab !== ALL_TAB) && <SourceTabs tabs={tabs} tab={tab} onTab={onTab} />}

      {visible.length > 0 &&
        (selected.size > 0 ? (
          <div className="ifiles__bar ifiles__bar--sel">
            <span className="ifiles__bar-text" title="폴더로 끌어다 놓거나, 폴더의 [여기에 넣기]나 숫자 키로 넣어요">
              <b>{selected.size}개 고름</b> · 숫자 키로도 넣어요
            </span>
            <button
              type="button"
              className="ifiles__bar-btn ifiles__bar-btn--icon"
              onClick={props.onTrash}
              aria-label="고른 파일을 휴지통으로"
              title="고른 파일을 휴지통으로(되돌리기 가능)"
            >
              <Trash2 size={15} />
            </button>
            <button type="button" className="ifiles__bar-btn" onClick={onClear}>
              고르기 풀기
            </button>
          </div>
        ) : (
          <div className="ifiles__bar">
            <button
              type="button"
              className="ifiles__bar-btn ifiles__all"
              onClick={() => onSelectMany(visible.map((i) => i.path))}
              title="파일을 누르면 하나씩, Shift를 누른 채 누르면 사이의 파일까지 골라져요"
            >
              {allVisibleSelected ? "고르기 풀기" : "모두 고르기"}
            </button>
            <span className="ifiles__spacer" />
            <button
              type="button"
              className="ifiles__accept-all"
              onClick={() => props.onClassify(visible.map((i) => i.path))}
              disabled={props.busy}
              title="파일 이름과 형식을 보고 폴더로 나눠요(먼저 보여 드리고 확인하면 넣어요)"
            >
              <Wand2 size={14} aria-hidden="true" />
              자동 분류
            </button>
          </div>
        ))}

      {showRecent ? (
        props.recent
      ) : visible.length > 0 ? (
        <>
        <ul className="ifiles__list">
          {groups.map((g) =>
            g.items.length === 0 ? null : (
              <Fragment key={g.key}>
                <li className="ifiles__group" role="presentation">
                  <Band tone={g.tone} title={g.title} count={g.items.length} hint={g.hint} />
                </li>
                {g.items.map((item) => (
                  <FileRow
                    key={item.path}
                    item={item}
                    source={friendlyPlace(item.sourceDir, known)}
                    checked={selected.has(item.path)}
                    dimmed={!!props.dragging?.includes(item.path)}
                    onToggle={(shift) => pick(item.path, shift)}
                    suggestion={
                      item.suggestion ? displayPath(props.root, item.suggestion.destDir, props.separator) : null
                    }
                    onAccept={() => props.onAccept([item.path])}
                    busy={props.busy}
                  />
                ))}
              </Fragment>
            ),
          )}
        </ul>
        {/* 처음에 한 번쯤 쓰는 버튼이라 맨 아래 한 줄에 고정한다(목록만 그 위에서 내려감, 사용자 요청).
            목록이 비면 가운데에 크게. */}
        <div className="ifiles__more">
          <span className="ifiles__more-text">원래 있던 파일도</span>
          {importButton}
        </div>
        </>
      ) : (
        <div className="ifiles__empty">
          <div className="ifiles__empty-title">정리함이 비었어요</div>
          <div className="ifiles__empty-text">
            새 파일이 들어오면 여기에 모여요. 다운로드·바탕화면에 원래 있던 파일도 불러올 수 있어요.
          </div>
          {importButton}
        </div>
      )}
    </section>
  );
}

/**
 * 폴더 탭(전체·다운로드·바탕화면…). 늘 한 줄이고, 자리가 모자라면 들어가는 만큼만 보이고
 * 나머지는 끝의 [더 보기 n ▾]에 넣는다. 고른 탭은 늘 보인다.
 */
function SourceTabs({
  tabs,
  tab,
  onTab,
}: {
  tabs: { key: string; label: string; count: number }[];
  tab: string;
  onTab: (tab: string) => void;
}) {
  const row = useRef<HTMLDivElement>(null);
  const ruler = useRef<HTMLDivElement>(null);
  const [fit, setFit] = useState(tabs.length);
  const [open, setOpen] = useState(false);
  const sig = tabs.map((t) => `${t.key}\u0000${t.label}\u0000${t.count}`).join("\u0001");

  // 숨긴 자(ruler)에 모든 탭과 [더 보기]를 그려 너비를 재고, 줄에 들어가는 개수를 정한다
  useLayoutEffect(() => {
    const el = row.current;
    const r = ruler.current;
    if (!el || !r) return;
    const GAP = 2;
    const calc = () => {
      const style = getComputedStyle(el);
      const avail = el.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      const widths = [...r.children].map((c) => (c as HTMLElement).offsetWidth);
      const more = widths.pop() ?? 0;
      const all = widths.reduce((a, w) => a + w + GAP, -GAP);
      if (all <= avail) {
        setFit(widths.length);
        return;
      }
      let used = 0;
      let n = 0;
      for (const w of widths) {
        if (used + w + GAP + more > avail) break;
        used += w + GAP;
        n += 1;
      }
      setFit(Math.max(1, n));
    };
    calc();
    const ro = new ResizeObserver(calc);
    ro.observe(el);
    return () => ro.disconnect();
  }, [sig]);

  // 메뉴는 바깥을 누르거나 Esc로 닫는다
  useEffect(() => {
    if (!open) return;
    const close = (e: PointerEvent) => {
      if (!row.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);

  let shown = tabs.slice(0, fit);
  // 고른 탭이 [더 보기] 안에 있으면 마지막 자리에 꺼내 보인다
  if (!shown.some((t) => t.key === tab)) {
    const cur = tabs.find((t) => t.key === tab);
    if (cur) shown = [...shown.slice(0, -1), cur];
  }
  const hidden = tabs.filter((t) => !shown.includes(t));

  const label = (t: { label: string; count: number }) => (
    <>
      {t.label} <span className="ifiles__src-count">{t.count}</span>
    </>
  );

  return (
    <div className="ifiles__src" ref={row} role="tablist" aria-label="보낸 폴더">
      <div className="ifiles__src-ruler" ref={ruler} aria-hidden="true">
        {tabs.map((t) => (
          <span key={t.key} className="ifiles__src-tab">
            {label(t)}
          </span>
        ))}
        <span className="ifiles__src-tab">
          더 보기 <span className="ifiles__src-count">{tabs.length}</span>
          <ChevronDown size={13} />
        </span>
      </div>
      {shown.map((t) => (
        <button
          key={t.key}
          type="button"
          role="tab"
          aria-selected={tab === t.key}
          className={`ifiles__src-tab${tab === t.key ? " ifiles__src-tab--on" : ""}`}
          onClick={() => onTab(t.key)}
        >
          {label(t)}
        </button>
      ))}
      {hidden.length > 0 && (
        <button
          type="button"
          className="ifiles__src-tab ifiles__src-more"
          aria-haspopup="menu"
          aria-expanded={open}
          onClick={() => setOpen((o) => !o)}
        >
          더 보기 <span className="ifiles__src-count">{hidden.length}</span>
          <ChevronDown size={13} aria-hidden="true" />
        </button>
      )}
      {open && hidden.length > 0 && (
        <div className="ifiles__src-menu" role="menu">
          {hidden.map((t) => (
            <button
              key={t.key}
              type="button"
              role="menuitem"
              onClick={() => {
                onTab(t.key);
                setOpen(false);
              }}
            >
              <span>{t.label}</span>
              <span className="ifiles__src-count">{t.count}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function FileRow({
  item,
  source,
  checked,
  dimmed,
  onToggle,
  suggestion,
  onAccept,
  busy,
}: {
  item: InboxItem;
  source: string;
  checked: boolean;
  dimmed: boolean;
  /** shift = Shift를 누른 채 눌렀는지(사이의 파일까지 고르기) */
  onToggle: (shift: boolean) => void;
  /** 추천 위치(보여 줄 경로) */
  suggestion: string | null;
  onAccept: () => void;
  busy: boolean;
}) {
  const { attributes, listeners, setNodeRef } = useDraggable({ id: item.path, data: { item } });
  return (
    <li
      ref={setNodeRef}
      className={`ifiles__row${checked ? " ifiles__row--on" : ""}${dimmed ? " ifiles__row--drag" : ""}`}
      {...attributes}
      {...listeners}
      onClick={(e) => onToggle(e.shiftKey)}
      // 키보드: 스페이스로 고르기(끌기 대신)
      onKeyDown={(e) => {
        if (e.key === " " || e.key === "Enter") {
          e.preventDefault();
          onToggle(e.shiftKey);
        }
      }}
      aria-pressed={checked}
      aria-label={`${item.name} ${checked ? "고름" : "고르기"}`}
    >
      <div className="ifiles__main">
        <div className="ifiles__top">
          <FileBadge name={item.name} size={36} />
          <div className="ifiles__text">
            <div className="ifiles__name" title={item.name}>
              {item.name}
            </div>
            <div className="ifiles__meta">
              {source} · {formatAgo(item.addedAt)}
            </div>
          </div>
        </div>
        {suggestion ? (
          <div className="ifiles__sug">
            <span className="ifiles__sug-path" title={`추천 위치: ${suggestion}`}>
              <ArrowRight size={12} aria-hidden="true" />
              <span>{suggestion}</span>
            </span>
            <button
              type="button"
              className="ifiles__sug-put"
              onClick={(e) => {
                e.stopPropagation();
                onAccept();
              }}
              onPointerDown={(e) => e.stopPropagation()}
              onKeyDown={(e) => e.stopPropagation()}
              disabled={busy}
              aria-label={`${item.name} 추천 위치에 넣기`}
            >
              넣기
            </button>
          </div>
        ) : (
          <div className="ifiles__nosug">추천 없음 · 오른쪽 폴더로 끌어다 놓으세요</div>
        )}
      </div>
    </li>
  );
}
