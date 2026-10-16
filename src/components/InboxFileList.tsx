import { useDraggable } from "@dnd-kit/core";
import { Download } from "lucide-react";
import { formatAgo } from "../lib/format";
import { friendlyName, samePath, type KnownFolders } from "../lib/paths";
import type { InboxItem } from "../lib/types";
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
  onSelectMany: (paths: string[]) => void;
  onClear: () => void;
  dragging: string[] | null;
  onImport: () => void;
  importing: boolean;
}

export const ALL_TAB = "all";

/** 정리함 왼쪽 파일 칸(SPEC 3-2): 탭, 체크박스·배지·이름·출처와 시각. 줄을 끌어 폴더에 놓을 수 있다. */
export default function InboxFileList(props: Props) {
  const { items, watchFolders, known, tab, onTab, selected, onToggle, onSelectMany, onClear } = props;

  const tabs = [
    { key: ALL_TAB, label: "전체", count: items.length },
    ...watchFolders.map((dir) => ({
      key: dir,
      label: friendlyName(dir, known),
      count: items.filter((i) => samePath(i.sourceDir, dir)).length,
    })),
  ];
  const visible = tab === ALL_TAB ? items : items.filter((i) => samePath(i.sourceDir, tab));
  const allVisibleSelected = visible.length > 0 && visible.every((i) => selected.has(i.path));

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
        <div className="ifiles__tabs" role="tablist">
          {tabs.map((t) => (
            <button
              key={t.key}
              type="button"
              role="tab"
              aria-selected={tab === t.key}
              className={`ifiles__tab${tab === t.key ? " ifiles__tab--on" : ""}`}
              onClick={() => onTab(t.key)}
            >
              {t.label} <span className="ifiles__count">{t.count}</span>
            </button>
          ))}
        </div>
      </div>

      {visible.length > 0 &&
        (selected.size > 0 ? (
          <div className="ifiles__bar ifiles__bar--sel">
            <span className="ifiles__bar-text">{selected.size}개 선택됨 · ‘여기에 넣기’나 숫자 키로 넣어요</span>
            <button type="button" className="ifiles__bar-btn" onClick={onClear}>
              선택 해제
            </button>
          </div>
        ) : (
          <div className="ifiles__bar">
            <label className="ifiles__all">
              <input
                type="checkbox"
                checked={allVisibleSelected}
                onChange={() => onSelectMany(visible.map((i) => i.path))}
              />
              모두 선택
            </label>
            <span className="ifiles__spacer" />
            {importButton}
          </div>
        ))}

      {visible.length > 0 ? (
        <ul className="ifiles__list">
          {visible.map((item) => (
            <FileRow
              key={item.path}
              item={item}
              source={friendlyName(item.sourceDir, known)}
              checked={selected.has(item.path)}
              dimmed={!!props.dragging?.includes(item.path)}
              onToggle={() => onToggle(item.path)}
            />
          ))}
        </ul>
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

function FileRow({
  item,
  source,
  checked,
  dimmed,
  onToggle,
}: {
  item: InboxItem;
  source: string;
  checked: boolean;
  dimmed: boolean;
  onToggle: () => void;
}) {
  const { attributes, listeners, setNodeRef } = useDraggable({ id: item.path, data: { item } });
  return (
    <li
      ref={setNodeRef}
      className={`ifiles__row${checked ? " ifiles__row--on" : ""}${dimmed ? " ifiles__row--drag" : ""}`}
      {...attributes}
      {...listeners}
      onClick={onToggle}
      // 키보드: 스페이스로 선택(끌기 대신)
      onKeyDown={(e) => {
        if (e.key === " " || e.key === "Enter") {
          e.preventDefault();
          onToggle();
        }
      }}
      aria-pressed={checked}
      aria-label={`${item.name} ${checked ? "선택됨" : "선택"}`}
    >
      <input
        type="checkbox"
        tabIndex={-1}
        aria-hidden="true"
        checked={checked}
        onChange={onToggle}
        onClick={(e) => e.stopPropagation()}
        onPointerDown={(e) => e.stopPropagation()}
      />
      <FileBadge name={item.name} size={36} />
      <div className="ifiles__text">
        <div className="ifiles__name" title={item.name}>
          {item.name}
        </div>
        <div className="ifiles__meta">
          {source} · {formatAgo(item.addedAt)}
        </div>
      </div>
    </li>
  );
}
