import { useCallback, useEffect, useState } from "react";
import { ArrowRight, Undo2 } from "lucide-react";
import { errorMessage, listRecentMoves, onBackendEvent, openPath, revealPath, undoMoves } from "../lib/backend";
import { formatAgo, withRo } from "../lib/format";
import { displayPath, friendlyName, trailFor, type KnownFolders } from "../lib/paths";
import type { RecentMove } from "../lib/types";
import FileBadge from "./FileBadge";
import type { Toast } from "./FolderPane";
import "./RecentMoveList.css";

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
        {moves.map((m) => (
          <li key={m.id}>
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
                <span className="recent__when">{formatAgo(m.movedAt)}</span>
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
        ))}
      </ul>
    </>
  );
}
