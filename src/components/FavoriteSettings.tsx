import { useEffect, useState } from "react";
import { getFavoriteFolders, onBackendEvent } from "../lib/backend";
import { baseName, displayPath } from "../lib/paths";
import Dialog from "./Dialog";
import FolderPicker from "./FolderPicker";
import "./FavoriteSettings.css";

const SLOTS = 6;

interface Props {
  root: string;
  separator: string;
  /** 직접 고른 폴더(칸마다, null = 자동) */
  pins: (string | null)[];
  onChange: (pins: (string | null)[]) => Promise<void>;
}

/**
 * 자주 쓰는 폴더(숫자 키 1~6) 직접 고르기(SPEC 3-3 설정).
 * 고르지 않은 칸은 지금처럼 자동(많이 넣은 폴더부터)으로 채운다.
 */
export default function FavoriteSettings({ root, separator, pins, onChange }: Props) {
  const [current, setCurrent] = useState<string[]>([]);
  const [picking, setPicking] = useState<number | null>(null);
  const [choice, setChoice] = useState<string | null>(null);

  useEffect(() => {
    const load = () => void getFavoriteFolders().then(setCurrent).catch(() => setCurrent([]));
    load();
    return onBackendEvent("inbox-changed", load);
  }, [pins]);

  const update = (slot: number, value: string | null) => {
    const next = Array.from({ length: SLOTS }, (_, i) => pins[i] ?? null);
    next[slot] = value;
    while (next.length > 0 && next[next.length - 1] === null) next.pop();
    return onChange(next);
  };

  return (
    <>
      <ol className="favs">
        {Array.from({ length: SLOTS }, (_, i) => {
          const pin = pins[i] ?? null;
          const auto = current[i];
          return (
            <li key={i} className="favs__row">
              <span className="favs__key">{i + 1}</span>
              <span className="favs__name">
                {pin ? (
                  <strong title={pin}>{displayPath(root, pin, separator)}</strong>
                ) : (
                  <span className="favs__auto" title={auto}>
                    자동{auto ? ` · 지금은 ‘${baseName(auto)}’` : ""}
                  </span>
                )}
              </span>
              <div className="button-row favs__actions">
                {pin && (
                  <button type="button" className="btn btn--ghost" onClick={() => void update(i, null)}>
                    자동으로
                  </button>
                )}
                <button
                  type="button"
                  className="btn"
                  onClick={() => {
                    setChoice(pin);
                    setPicking(i);
                  }}
                >
                  고르기
                </button>
              </div>
            </li>
          );
        })}
      </ol>

      {picking !== null && (
        <Dialog
          wide
          title={`${picking + 1}번 자주 쓰는 폴더 고르기`}
          onClose={() => setPicking(null)}
          actions={
            <>
              <button type="button" className="btn btn--ghost" onClick={() => setPicking(null)}>
                취소
              </button>
              <button
                type="button"
                className="btn btn--primary"
                disabled={!choice}
                onClick={() => {
                  const slot = picking;
                  setPicking(null);
                  void update(slot, choice);
                }}
              >
                이 폴더로
              </button>
            </>
          }
        >
          <p className="folder-move__target">
            고른 폴더 <strong>{choice ? displayPath(root, choice, separator) : "아래에서 폴더를 골라 주세요"}</strong>
          </p>
          <FolderPicker root={root} separator={separator} selected={choice} onSelect={setChoice} reveal={choice} />
        </Dialog>
      )}
    </>
  );
}
