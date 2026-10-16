import { Inbox, Settings as SettingsIcon } from "lucide-react";
import { useEffect, useState } from "react";
import FolderIcon from "../components/FolderIcon";
import UpdateBanner from "../components/UpdateBanner";
import { listInbox, onBackendEvent } from "../lib/backend";
import { friendlyName } from "../lib/paths";
import { useAppStore, type Page } from "../store/useAppStore";
import { startUpdateChecks } from "../store/useUpdateStore";
import InboxPage from "./InboxPage";
import SettingsPage from "./SettingsPage";
import "./MainWindow.css";

const MENU: { page: Page; label: string; Icon: typeof Inbox }[] = [
  { page: "inbox", label: "정리함", Icon: Inbox },
  { page: "settings", label: "설정", Icon: SettingsIcon },
];

// 메인 창(SPEC 3-2): 왼쪽 메뉴 + 오른쪽 내용. 메뉴는 기능이 생기는 단계마다 늘어난다.
export default function MainWindow() {
  const page = useAppStore((s) => s.page);
  const goTo = useAppStore((s) => s.goTo);
  const settings = useAppStore((s) => s.settings);
  const known = useAppStore((s) => s.setupInfo);

  const [inboxCount, setInboxCount] = useState(0);

  useEffect(() => startUpdateChecks(), []);

  // 왼쪽 메뉴 "정리함" 옆 개수
  useEffect(() => {
    const load = () => void listInbox().then((l) => setInboxCount(l.length)).catch(() => {});
    load();
    return onBackendEvent("inbox-changed", load);
  }, []);

  const watching = settings?.watchFolders.map((w) => friendlyName(w, known)).join(" · ") ?? "";

  return (
    <div className="main-window">
      <nav className="sidebar" aria-label="메뉴">
        <div className="sidebar__brand">
          <span className="sidebar__logo" aria-hidden="true">
            <FolderIcon size={18} />
          </span>
          <span className="sidebar__brand-text">파일 정리 도우미</span>
        </div>
        {MENU.map(({ page: p, label, Icon }) => (
          <button
            key={p}
            type="button"
            className={`sidebar__item${page === p ? " sidebar__item--on" : ""}`}
            title={label}
            aria-current={page === p ? "page" : undefined}
            onClick={() => goTo(p)}
          >
            <Icon size={18} aria-hidden="true" />
            <span className="sidebar__label">{label}</span>
            {p === "inbox" && inboxCount > 0 && <span className="sidebar__badge">{inboxCount}</span>}
          </button>
        ))}
        <div className="sidebar__spacer" />
        <div className="sidebar__status" title={`지켜볼 폴더: ${watching}`}>
          <div className="sidebar__status-title">
            <span className="sidebar__dot" aria-hidden="true" />
            <span>{settings?.organizeMode === "manual" ? "모아 두고 나중에 정리" : "바로 물어보기"}</span>
          </div>
          <div className="sidebar__status-text">지켜볼 폴더: {watching}</div>
        </div>
      </nav>
      <main className="main-window__content">
        <UpdateBanner />
        {page === "inbox" ? <InboxPage /> : <SettingsPage />}
      </main>
    </div>
  );
}
