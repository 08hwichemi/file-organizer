import { useEffect, useState } from "react";
import FileBadge from "../components/FileBadge";
import FolderGrid from "../components/FolderGrid";
import { listInbox, onBackendEvent } from "../lib/backend";
import { formatAgo } from "../lib/format";
import { friendlyName } from "../lib/paths";
import type { InboxItem } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import "./InboxPage.css";

// 정리함(SPEC 3-2). 3단계: 모인 파일 목록 + 정리 폴더 탐색. 넣는 방법(끌어다 놓기 등)은 4단계.
export default function InboxPage() {
  const settings = useAppStore((s) => s.settings);
  const known = useAppStore((s) => s.setupInfo);
  const root = settings?.rootFolder;
  const [items, setItems] = useState<InboxItem[] | null>(null);

  useEffect(() => {
    const load = () => void listInbox().then(setItems).catch(() => setItems([]));
    load();
    return onBackendEvent("inbox-changed", load);
  }, []);

  return (
    <div className="page">
      <div>
        <h1 className="page__title">정리함</h1>
        <p className="page__lead">
          {settings?.organizeMode === "manual"
            ? "새 파일은 여기에 모여요. 원할 때 오른쪽 폴더로 넣어 정리하세요."
            : "새 파일이 생기면 바로 물어보고, ‘나중에 정리’한 파일은 여기에 모여요."}
        </p>
      </div>
      <div className="inbox">
        <section className="inbox__files" aria-label="정리할 파일">
          {items && items.length > 0 && (
            <>
              <div className="inbox__files-head">정리할 파일 {items.length}개</div>
              <ul className="inbox__list">
                {items.map((item) => (
                  <li key={item.id} className="inbox__row">
                    <FileBadge name={item.name} size={36} />
                    <div className="inbox__text">
                      <div className="inbox__name" title={item.name}>
                        {item.name}
                      </div>
                      <div className="inbox__meta">
                        {friendlyName(item.sourceDir, known)} · {formatAgo(item.addedAt)}
                      </div>
                    </div>
                  </li>
                ))}
              </ul>
            </>
          )}
          {items && items.length === 0 && (
            <div className="inbox__empty">
              <div className="inbox__empty-title">정리함이 비었어요</div>
              <div className="inbox__empty-text">새 파일이 들어오면 여기에 모여요.</div>
            </div>
          )}
        </section>
        {root && (
          <div className="inbox__folders">
            <FolderGrid key={root} rootPath={root} />
          </div>
        )}
      </div>
    </div>
  );
}
