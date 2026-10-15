import { useEffect, useState } from "react";
import { getAppInfo, type AppInfo } from "./lib/backend";
import "./App.css";

function osLabel(os: string): string {
  if (os === "windows") return "Windows";
  if (os === "browser") return "브라우저 미리보기";
  return os;
}

// 1단계(뼈대): 설치 후 창이 뜨는지만 확인하는 빈 화면.
// 정리함(3-2) 화면은 4단계에서 이 자리에 들어온다.
export default function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    getAppInfo()
      .then(setInfo)
      .catch(() => setError(true));
  }, []);

  return (
    <main className="empty">
      <div className="empty__card">
        <svg className="empty__icon" aria-hidden="true" viewBox="0 0 64 52">
          <path
            d="M4 8a5 5 0 0 1 5-5h14l6 6h26a5 5 0 0 1 5 5v4H4z"
            fill="var(--color-folder-back)"
          />
          <rect x="2" y="15" width="60" height="35" rx="5" fill="var(--color-folder-front)" />
        </svg>
        <h1 className="empty__title">파일 정리 도우미</h1>
        <p className="empty__text">
          프로그램이 잘 설치되었어요. 정리 기능은 다음 버전부터 하나씩 들어옵니다.
        </p>
        <p className="empty__meta" data-testid="app-info">
          {error && "프로그램 정보를 읽지 못했어요"}
          {!error && info && `버전 ${info.version} · ${osLabel(info.os)}`}
        </p>
      </div>
    </main>
  );
}
