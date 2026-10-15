import { useEffect, useState } from "react";
import { isPopupWindow } from "./lib/backend";
import { useAppStore } from "./store/useAppStore";
import FirstRun from "./screens/FirstRun";
import MainWindow from "./screens/MainWindow";
import Popup from "./screens/Popup";

export default function App() {
  const status = useAppStore((s) => s.status);
  const loadError = useAppStore((s) => s.loadError);
  const load = useAppStore((s) => s.load);
  const [popup, setPopup] = useState<boolean | null>(null);

  useEffect(() => {
    void isPopupWindow().then(setPopup);
    void load();
  }, [load]);

  if (status === "loading" || popup === null) return null;
  if (status === "error") {
    return (
      <main className="center-message" role="alert">
        <p>설정을 불러오지 못했어요.</p>
        <p className="center-message__detail">{loadError}</p>
      </main>
    );
  }
  if (popup) return <Popup />;
  return status === "setup" ? <FirstRun /> : <MainWindow />;
}
