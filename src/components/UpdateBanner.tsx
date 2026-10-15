import { Download } from "lucide-react";
import { useUpdateStore } from "../store/useUpdateStore";
import "./UpdateBanner.css";

/** "새 버전이 있어요 · 지금 업데이트" 띠. 새 버전이 있을 때만 보인다. */
export default function UpdateBanner() {
  const { status, version, progress, error, install } = useUpdateStore();
  if (status !== "available" && status !== "downloading") return null;

  return (
    <div className="update-banner" role="status">
      <Download size={18} aria-hidden="true" className="update-banner__icon" />
      <div className="update-banner__text">
        {status === "downloading" ? (
          <>
            <strong>새 버전을 설치하는 중이에요</strong>
            <span>
              {progress !== null ? `${progress}% · ` : ""}끝나면 프로그램이 알아서 다시 켜져요.
            </span>
          </>
        ) : (
          <>
            <strong>새 버전 {version}이 있어요</strong>
            <span>{error ?? "설치하는 동안 잠깐 꺼졌다가 다시 켜져요."}</span>
          </>
        )}
      </div>
      {status === "available" && (
        <button type="button" className="btn btn--accent" onClick={() => void install()}>
          지금 업데이트
        </button>
      )}
    </div>
  );
}
