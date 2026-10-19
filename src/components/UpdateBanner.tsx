import { CircleCheck, Download, X } from "lucide-react";
import { useUpdateStore } from "../store/useUpdateStore";
import "./UpdateBanner.css";

/**
 * 업데이트 띠(SPEC 7장). 새 버전이 있거나(직접 누르기), 받아 두었거나(자동: 창을 닫으면 설치),
 * 설치 중일 때, 그리고 업데이트 뒤 다시 켜졌을 때("0.1.35로 업데이트했어요") 보인다.
 */
export default function UpdateBanner() {
  const { status, version, progress, error, install, updatedTo, dismissUpdated } = useUpdateStore();

  if (status === "downloading" || status === "installing") {
    return (
      <div className="update-banner" role="status">
        <Download size={18} aria-hidden="true" className="update-banner__icon" />
        <div className="update-banner__text">
          <strong>새 버전을 설치하는 중이에요</strong>
          <span>
            {status === "downloading" && progress !== null ? `${progress}% · ` : ""}끝나면 프로그램이 알아서 다시
            켜져요.
          </span>
        </div>
      </div>
    );
  }

  if (status === "available" || status === "ready") {
    return (
      <div className="update-banner" role="status">
        <Download size={18} aria-hidden="true" className="update-banner__icon" />
        <div className="update-banner__text">
          <strong>{status === "ready" ? `새 버전(${version})을 받아 두었어요` : `새 버전(${version})이 나왔어요`}</strong>
          <span>
            {error ??
              (status === "ready"
                ? "창을 닫으면 알아서 설치하고 다시 켜져요."
                : "설치하는 동안 잠깐 꺼졌다가 다시 켜져요.")}
          </span>
        </div>
        <button type="button" className="btn btn--accent" onClick={() => void install()}>
          지금 업데이트
        </button>
      </div>
    );
  }

  if (updatedTo) {
    return (
      <div className="update-banner update-banner--done" role="status">
        <CircleCheck size={18} aria-hidden="true" className="update-banner__icon" />
        <div className="update-banner__text">
          <strong>{updatedTo} 버전으로 업데이트했어요</strong>
          <span>프로그램을 쓰지 않을 때 알아서 설치했어요. 하던 설정과 기록은 그대로예요.</span>
        </div>
        <button type="button" className="icon-btn update-banner__close" aria-label="닫기" onClick={dismissUpdated}>
          <X size={16} />
        </button>
      </div>
    );
  }

  return null;
}
