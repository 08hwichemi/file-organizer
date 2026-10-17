import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import { ArrowRight, Download, Search, Undo2, X } from "lucide-react";
import Band from "../components/Band";
import FileBadge from "../components/FileBadge";
import { errorMessage, listMoveLog, onBackendEvent, restoreDeleted, revealPath, saveLogCsv, undoMoves } from "../lib/backend";
import { friendlyName, trailFor } from "../lib/paths";
import type { LogEntry } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import "./LogPage.css";

type Period = "all" | "today" | "week" | "month";
const PERIODS: { key: Period; label: string }[] = [
  { key: "all", label: "전체" },
  { key: "today", label: "오늘" },
  { key: "week", label: "이번 주" },
  { key: "month", label: "이번 달" },
];
/** 한 번에 그리는 줄 수(더 있으면 [더 보기]) */
const PAGE = 200;
const WEEKDAYS = ["일", "월", "화", "수", "목", "금", "토"];

const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/** 기간의 시작 시각(이번 주는 월요일부터) */
function periodStart(period: Period, now = new Date()): number {
  if (period === "today") return startOfDay(now);
  if (period === "week") return startOfDay(now) - ((now.getDay() + 6) % 7) * 86_400_000;
  if (period === "month") return new Date(now.getFullYear(), now.getMonth(), 1).getTime();
  return 0;
}

/** 날짜 띠: 오늘 · 어제 · 10월 14일 (화) · 2025년 3월 2일 (일) */
function dayLabel(ms: number, now = new Date()): string {
  const today = startOfDay(now);
  if (ms >= today) return "오늘";
  if (ms >= today - 86_400_000) return "어제";
  const d = new Date(ms);
  const date = `${d.getMonth() + 1}월 ${d.getDate()}일 (${WEEKDAYS[d.getDay()]})`;
  return d.getFullYear() === now.getFullYear() ? date : `${d.getFullYear()}년 ${date}`;
}

function timeText(ms: number): string {
  const d = new Date(ms);
  const h = d.getHours();
  return `${h < 12 ? "오전" : "오후"} ${h % 12 === 0 ? 12 : h % 12}:${String(d.getMinutes()).padStart(2, "0")}`;
}

const dirOf = (path: string) => path.slice(0, Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/")));
const nameOf = (path: string) => path.slice(Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/")) + 1);

/** 기록 줄의 상태(오른쪽 칸·엑셀의 '상태') */
function status(e: LogEntry): string {
  if (e.kind === "trash") return e.revertedAt ? "되살림" : "휴지통에 있음";
  if (e.revertedAt) return "되돌림";
  return e.present ? "" : "다시 옮김";
}

const KIND_LABEL: Record<LogEntry["kind"], string> = { move: "넣기", rename: "이름 바꾸기", trash: "휴지통" };

/** 엑셀에서 열 수 있게 CSV로(칸마다 큰따옴표) */
function toCsv(list: LogEntry[]): string {
  const cell = (v: string) => `"${v.replace(/"/g, '""')}"`;
  const pad = (n: number) => String(n).padStart(2, "0");
  const lines = [["날짜", "시각", "종류", "이름", "원래 있던 곳", "옮긴 곳", "상태"].map(cell).join(",")];
  for (const e of list) {
    const d = new Date(e.at);
    lines.push(
      [
        `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`,
        `${pad(d.getHours())}:${pad(d.getMinutes())}`,
        KIND_LABEL[e.kind],
        e.name,
        e.kind === "rename" ? e.fromPath : dirOf(e.fromPath),
        e.kind === "trash" ? "휴지통" : e.kind === "rename" ? (e.toPath ?? "") : dirOf(e.toPath ?? ""),
        status(e),
      ]
        .map(cell)
        .join(","),
    );
  }
  return lines.join("\r\n") + "\r\n";
}

/**
 * 정리 기록(SPEC 3-2, 사용자 요청): AutoFolder로 넣고 옮긴 파일을 모두 — 언제, 어디에서, 어디로.
 * 되돌린 것·휴지통에 보낸 것도 남는다. 찾기·기간, [엑셀 파일로 저장]. 기록은 이 PC에만 있다.
 */
export default function LogPage() {
  const settings = useAppStore((s) => s.settings);
  const known = useAppStore((s) => s.setupInfo);
  const root = settings?.rootFolder ?? null;
  const sep = known?.separator ?? "\\";

  const [entries, setEntries] = useState<LogEntry[] | null>(null);
  const [query, setQuery] = useState("");
  const [period, setPeriod] = useState<Period>("all");
  const [shown, setShown] = useState(PAGE);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const reload = useCallback(() => {
    void listMoveLog()
      .then(setEntries)
      .catch((e) => {
        setEntries([]);
        setNotice(errorMessage(e));
      });
  }, []);

  useEffect(() => {
    reload();
    const stops = [onBackendEvent("inbox-changed", reload), onBackendEvent("folders-changed", reload)];
    return () => stops.forEach((stop) => stop());
  }, [reload]);

  useEffect(() => {
    if (!notice) return;
    const t = window.setTimeout(() => setNotice(null), 6000);
    return () => window.clearTimeout(t);
  }, [notice]);

  // 정리 폴더 안은 "업무 › 공문"처럼 정리 폴더 이름을 빼고 짧게, 그 밖은 "다운로드"·"바탕화면 › 생기부"처럼
  const where = useCallback(
    (dir: string) => {
      const trail = root ? trailFor(root, dir, sep) : null;
      if (trail) return trail.length === 1 ? trail[0].name : trail.slice(1).map((c) => c.name).join(" › ");
      return friendlyName(dir, known);
    },
    [root, sep, known],
  );

  const filtered = useMemo(() => {
    if (!entries) return [];
    const since = periodStart(period);
    const q = query.trim().toLowerCase();
    return entries.filter(
      (e) =>
        e.at >= since &&
        (!q ||
          e.name.toLowerCase().includes(q) ||
          where(dirOf(e.fromPath)).toLowerCase().includes(q) ||
          (e.toPath !== null && where(dirOf(e.toPath)).toLowerCase().includes(q))),
    );
  }, [entries, period, query, where]);

  // 검색어·기간을 바꾸면 처음부터
  useEffect(() => setShown(PAGE), [period, query]);

  const counts = useMemo(() => {
    const map = new Map<string, number>();
    for (const e of filtered) map.set(dayLabel(e.at), (map.get(dayLabel(e.at)) ?? 0) + 1);
    return map;
  }, [filtered]);

  const act = async (what: () => Promise<{ restored: number; errors: string[] }>, done: string) => {
    if (busy) return;
    setBusy(true);
    try {
      const r = await what();
      setNotice(r.errors.length === 0 ? done : r.errors[0]);
    } catch (e) {
      setNotice(errorMessage(e));
    } finally {
      setBusy(false);
      reload();
    }
  };

  const exportCsv = async () => {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    try {
      const saved = await saveLogCsv(
        `정리 기록 ${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}.csv`,
        toCsv(filtered),
      );
      if (saved) setNotice(`기록 ${filtered.length}줄을 엑셀(CSV) 파일로 저장했어요`);
    } catch (e) {
      setNotice(errorMessage(e));
    }
  };

  const list = filtered.slice(0, shown);

  return (
    <div className="page page--fill">
      <div className="log__head">
        <div>
          <h1 className="page__title">정리 기록</h1>
          <p className="page__lead">
            AutoFolder로 넣고 옮긴 파일을 모두 기록해 둬요. 언제, 어디에서, 어디로 옮겼는지 볼 수 있고, 기록은 이 PC에만
            저장돼요.
          </p>
        </div>
        <button
          type="button"
          className="btn log__export"
          onClick={() => void exportCsv()}
          disabled={filtered.length === 0}
          title="지금 보이는 기록(찾기·기간 적용)을 엑셀에서 열 수 있는 CSV 파일로 저장해요"
        >
          <Download size={16} aria-hidden="true" />
          엑셀 파일로 저장
        </button>
      </div>

      <section className="log" aria-label="정리 기록">
        <div className="log__bar">
          <label className="log__search">
            <Search size={16} aria-hidden="true" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && setQuery("")}
              placeholder="파일 이름이나 폴더로 찾기"
              aria-label="파일 이름이나 폴더로 찾기"
            />
            {query && (
              <button type="button" className="log__clear" onClick={() => setQuery("")} aria-label="찾기 지우기">
                <X size={15} />
              </button>
            )}
          </label>
          <div className="log__seg" role="group" aria-label="기간">
            {PERIODS.map((p) => (
              <button
                key={p.key}
                type="button"
                className={`log__seg-btn${period === p.key ? " log__seg-btn--on" : ""}`}
                aria-pressed={period === p.key}
                onClick={() => setPeriod(p.key)}
              >
                {p.label}
              </button>
            ))}
          </div>
        </div>

        {notice && (
          <p className="log__notice" role="status">
            {notice}
          </p>
        )}

        {entries && filtered.length === 0 ? (
          <div className="log__empty">
            {entries.length === 0
              ? "아직 기록이 없어요. 파일을 폴더에 넣으면 여기에 남아요."
              : "찾는 기록이 없어요. 검색어나 기간을 바꿔 보세요."}
          </div>
        ) : (
          <>
            <div className="log__cols" role="presentation">
              <span>시각</span>
              <span>파일</span>
              <span>원래 있던 곳 → 넣은 곳</span>
              <span />
            </div>
            <ul className="log__list">
              {list.map((e, i) => {
                const day = dayLabel(e.at);
                const first = i === 0 || dayLabel(list[i - 1].at) !== day;
                const s = status(e);
                return (
                  <Fragment key={`${e.kind}${e.id}`}>
                    {first && (
                      <li className="log__day" role="presentation">
                        <Band tone="gray" title={day} count={counts.get(day) ?? 0} />
                      </li>
                    )}
                    <li className={`log__row${s && s !== "휴지통에 있음" ? " log__row--off" : ""}`}>
                      <span className="log__time">{timeText(e.at)}</span>
                      <span className="log__file" title={e.name}>
                        <FileBadge name={e.name} size={30} />
                        <span className="log__name">{e.name}</span>
                      </span>
                      <span className="log__where">
                        {e.kind === "rename" ? (
                          <>
                            <span className="log__tag">이름 바꿈</span>
                            <span className="log__from" title={e.fromPath}>
                              {nameOf(e.fromPath)}
                            </span>
                            <ArrowRight size={13} aria-hidden="true" />
                            <b title={e.toPath ?? ""}>{e.name}</b>
                          </>
                        ) : (
                          <>
                            <span className="log__from" title={dirOf(e.fromPath)}>
                              {where(dirOf(e.fromPath))}
                            </span>
                            <ArrowRight size={13} aria-hidden="true" />
                            <b title={e.toPath ? dirOf(e.toPath) : "휴지통"}>
                              {e.toPath ? where(dirOf(e.toPath)) : "휴지통"}
                            </b>
                          </>
                        )}
                      </span>
                      <span className="log__acts">
                        {/* 휴지통에 있는 것은 곳 칸에 "→ 휴지통"이 보이므로 [되살리기]만 */}
                        {s && s !== "휴지통에 있음" && <span className="log__state">{s}</span>}
                        {e.kind !== "trash" && e.present && e.toPath && (
                          <button
                            type="button"
                            className="btn"
                            onClick={() => void revealPath(e.toPath ?? "").catch((err) => setNotice(errorMessage(err)))}
                            title="탐색기에서 이 파일이 있는 곳을 열어요"
                          >
                            위치
                          </button>
                        )}
                        {e.kind === "move" && e.present && e.id > 0 && (
                          <button
                            type="button"
                            className="btn"
                            disabled={busy}
                            onClick={() =>
                              void act(() => undoMoves([e.id]), `‘${e.name}’ 파일을 원래 있던 곳으로 되돌렸어요`)
                            }
                            title="원래 있던 곳으로 되돌려요"
                          >
                            <Undo2 size={14} aria-hidden="true" />
                            되돌리기
                          </button>
                        )}
                        {e.kind === "trash" && e.present && e.id > 0 && (
                          <button
                            type="button"
                            className="btn"
                            disabled={busy}
                            onClick={() => void act(() => restoreDeleted([e.id]), `‘${e.name}’ ${e.isFolder ? "폴더를" : "파일을"} 휴지통에서 되살렸어요`)}
                            title="휴지통에서 원래 있던 곳으로 되살려요"
                          >
                            되살리기
                          </button>
                        )}
                      </span>
                    </li>
                  </Fragment>
                );
              })}
            </ul>
            {filtered.length > shown && (
              <button type="button" className="btn log__more" onClick={() => setShown((n) => n + PAGE)}>
                더 보기 ({filtered.length - shown}개 더)
              </button>
            )}
          </>
        )}
      </section>
    </div>
  );
}
