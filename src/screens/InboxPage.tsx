import FolderGrid from "../components/FolderGrid";
import { useAppStore } from "../store/useAppStore";
import "./InboxPage.css";

// 정리함(SPEC 3-2). 2단계에서는 정리 폴더 탐색만 되고, 파일 목록은 3·4단계에서 채운다.
export default function InboxPage() {
  const settings = useAppStore((s) => s.settings);
  const root = settings?.rootFolder;

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
          <div className="inbox__empty">
            <div className="inbox__empty-title">정리함이 비었어요</div>
            <div className="inbox__empty-text">새 파일이 들어오면 여기에 모여요.</div>
          </div>
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
