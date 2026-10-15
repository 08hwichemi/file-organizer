import { useEffect } from "react";
import { useAppStore } from "./store/useAppStore";
import FirstRun from "./screens/FirstRun";
import MainWindow from "./screens/MainWindow";

export default function App() {
  const status = useAppStore((s) => s.status);
  const loadError = useAppStore((s) => s.loadError);
  const load = useAppStore((s) => s.load);

  useEffect(() => {
    void load();
  }, [load]);

  if (status === "loading") return null;
  if (status === "error") {
    return (
      <main className="center-message" role="alert">
        <p>설정을 불러오지 못했어요.</p>
        <p className="center-message__detail">{loadError}</p>
      </main>
    );
  }
  return status === "setup" ? <FirstRun /> : <MainWindow />;
}
