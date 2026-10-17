import { Fragment, useCallback, useEffect, useState } from "react";
import { ArrowRight, History, Undo2 } from "lucide-react";
import { errorMessage, listRecentMoves, onBackendEvent, openPath, revealPath, undoMoves } from "../lib/backend";
import { formatAgo, formatDate, withRo } from "../lib/format";
import { displayPath, friendlyName, trailFor, type KnownFolders } from "../lib/paths";
import type { RecentMove } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import Band from "./Band";
import FileBadge from "./FileBadge";
import type { Toast } from "./FolderPane";
import "./RecentMoveList.css";

/** 날짜 묶음: 오늘 · 어제 · 그 전(달력 날짜 기준) */
function dayLabel(ms: number, now = new Date()): string {
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  if (ms >= start) return "오늘";
  if (ms >= start - 86_400_000) return "어제";
  return "그 전";
}

/** 줄 아래 때: 오늘은 "3분 전", 어제는 "오후 3:20", 그 전은 "10월 14일 오후 3:20"(띠와 겹치지 않게) */
function whenText(ms: number, day: string): string {
  if (day === "오늘") return formatAgo(ms);
  const d = new Date(ms);
  const h = d.getHours();
  const time = `${h < 12 ? "오전" : "오후"} ${h % 12 === 0 ? 12 : h % 12}:${String(d.getMinutes()).padStart(2, "0")}`;
  return day === "어제" ? time : `${formatDate(ms)} ${time}`;
}

interface Props {
  root: string | null;
  separator: string;
  known: KnownFolders | null;
  /** 줄을 누르면 오른쪽 폴더 칸에서 그 폴더를 열고 파일을 골라 둔다 */
  onShow: (move: RecentMove) => void;
  onToast: (toast: Toast) => void;
}

/**
 * 정리함의 '최근 정리' 탭(SPEC 3-2): 최근에 넣거나 옮긴 파일, 어디서 어디로 갔는지, [되돌리기].
 * 기록은 이동할 때마다 이미 남기고 있어서(SPEC 5장) 그것을 보여 주기만 한다.
 */
export default function RecentMoveList({ root, separator, known, onShow, onToast }: Props) {
  const [moves, setMoves] = useState<RecentMove[] | null>(null);
  const goTo = useAppStore((s) => s.goTo);
  const [busy, setBusy] = useState(false);

  const reload = useCallback(() => {
    void listRecentMoves()
      .then(setMoves)
      .catch(() => setMoves([]));
  }, []);

  useEffect(() => {
    reload();
    const stops = [onBackendEvent("inbox-changed", reload), onBackendEvent("folders-changed", reload)];
    return () => stops.forEach((stop) => stop());
  }, [reload]);

  // 원래 있던 곳: 다운로드·바탕화면이면 그 이름, 정리 폴더 안이면 "정리 폴더 › 업무"처럼
  // 정리 폴더 안은 "업무 › 공문"처럼 정리 폴더 이름을 빼고 짧게(정리 폴더 자체는 그 이름)
  const inRoot = (dir: string) => {
    const trail = root ? trailFor(root, dir, separator) : null;
    if (!trail) return null;
    return trail.length === 1 ? trail[0].name : trail.slice(1).map((c) => c.name).join(" › ");
  };
  const from = (dir: string) => inRoot(dir) ?? friendlyName(dir, known);

  const undo = async (m: RecentMove) => {
    if (busy) return;
    setBusy(true);
    try {
      const r = await undoMoves([m.id]);
      onToast({
        message:
          r.errors.length === 0
            ? `‘${m.name}’ 파일을 ${withRo(from(m.originalDir).split(" › ").pop() ?? "")} 되돌렸어요`
            : r.errors[0],
      });
    } catch (e) {
      onToast({ message: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  };

  if (!moves) return null;
  if (moves.length === 0) {
    return (
      <div className="ifiles__empty">
        <div className="ifiles__empty-title">아직 정리한 파일이 없어요</div>
        <div className="ifiles__empty-text">파일을 폴더에 넣으면 여기에 어디로 갔는지 남아요. 되돌릴 수도 있어요.</div>
      </div>
    );
  }

  return (
    <>
      <p className="recent__lead">최근에 넣거나 옮긴 파일이에요. 누르면 오른쪽에서 그 폴더를 열어요.</p>
      <ul className="recent__list">
        {moves.map((m, i) => {
          const day = dayLabel(m.movedAt);
          const first = i === 0 || dayLabel(moves[i - 1].movedAt) !== day;
          return (
            <Fragment key={m.id}>
              {first && (
                <li className="recent__day" role="presentation">
                  <Band tone="gray" title={day} count={moves.filter((x) => dayLabel(x.movedAt) === day).length} />
                </li>
              )}
              <li>
                <div
                  role="button"
                  tabIndex={0}
                  className="recent__row"
                  onClick={() => onShow(m)}
                  onDoubleClick={() => void openPath(m.movedPath).catch((e) => onToast({ message: errorMessage(e) }))}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      onShow(m);
                    }
                  }}
                  title={`${m.name}\n누르면 오른쪽에서 그 폴더를 열어요. 두 번 누르면 파일이 열려요.`}
                >
                  <FileBadge name={m.name} size={34} />
                  <div className="recent__text">
                    <span className="recent__name">{m.name}</span>
                    <span className="recent__where">
                      <span className="recent__from">{from(m.originalDir)}</span>
                      <ArrowRight size={12} aria-hidden="true" />
                      <span className="recent__to" title={displayPath(root, m.destDir, separator)}>
                        {inRoot(m.destDir) ?? m.destDir}
                      </span>
                    </span>
                    <span className="recent__when">{whenText(m.movedAt, day)}</span>
                  </div>
                  <div className="recent__acts">
                    <button
                      type="button"
                      className="recent__btn"
                      onClick={(e) => {
                        e.stopPropagation();
                        void revealPath(m.movedPath).catch((err) => onToast({ message: errorMessage(err) }));
                      }}
                      title="탐색기에서 이 파일이 있는 곳을 열어요"
                    >
                      위치
                    </button>
                    <button
                      type="button"
                      className="recent__btn"
                      onClick={(e) => {
                        e.stopPropagation();
                        void undo(m);
                      }}
                      disabled={busy}
                      title="원래 있던 곳으로 되돌려요"
                    >
                      <Undo2 size={13} aria-hidden="true" />
                      되돌리기
                    </button>
                  </div>
                </div>
              </li>
            </Fragment>
          );
        })}
        {/* 최근 정리는 지금 그 자리에 있는 최근 파일만. 되돌린 것·휴지통까지 모두 보려면 정리 기록 화면으로 */}
        <li className="recent__all">
          <button type="button" className="btn" onClick={() => goTo("log")}>
            <History size={15} aria-hidden="true" />
            전체 기록 보기
          </button>
        </li>
      </ul>
    </>
  );
}
