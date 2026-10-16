import { Inbox, ListChecks, Pause, Play, Settings as SettingsIcon } from "lucide-react";
import { useEffect, useState } from "react";
import FolderIcon from "../components/FolderIcon";
import UpdateBanner from "../components/UpdateBanner";
import { getStatus, listInbox, onBackendEvent, setPaused } from "../lib/backend";
import { friendlyName } from "../lib/paths";
import { useAppStore, type Page } from "../store/useAppStore";
import { startUpdateChecks } from "../store/useUpdateStore";
import InboxPage from "./InboxPage";
import RulesPage from "./RulesPage";
import SettingsPage from "./SettingsPage";
import "./MainWindow.css";

const MENU: { page: Page; label: string; Icon: typeof Inbox }[] = [
  { page: "inbox", label: "정리함", Icon: Inbox },
  { page: "rules", label: "정리 규칙", Icon: ListChecks },
  { page: "settings", label: "설정", Icon: SettingsIcon },
];

// 메인 창(SPEC 3-2): 왼쪽 메뉴 + 오른쪽 내용. 메뉴는 기능이 생기는 단계마다 늘어난다.
export default function MainWindow() {
  const page = useAppStore((s) => s.page);
  const goTo = useAppStore((s) => s.goTo);
  const settings = useAppStore((s) => s.settings);
  const known = useAppStore((s) => s.setupInfo);

  const [inboxCount, setInboxCount] = useState(0);
  const [paused, setPausedState] = useState(false);

  useEffect(() => startUpdateChecks(), []);

  // 트레이 메뉴 "정리함" → 그 화면으로
  useEffect(
    () =>
      onBackendEvent("navigate", (page) => {
        if (page === "inbox" || page === "rules" || page === "settings") goTo(page);
      }),
    [goTo],
  );

  // 일시 정지(트레이에서 바꿔도 따라감)
  useEffect(() => {
    const load = () => void getStatus().then((s) => setPausedState(s.paused)).catch(() => {});
    load();
    return onBackendEvent("status-changed", load);
  }, []);

  const togglePause = () => void setPaused(!paused).then((s) => setPausedState(s.paused)).catch(() => {});

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
        <div
          className={`sidebar__status${paused ? " sidebar__status--paused" : ""}`}
          title={paused ? "일시 정지 중: 새 파일은 팝업 없이 정리함에 모여요" : `지켜볼 폴더: ${watching}`}
        >
          <div className="sidebar__status-title">
            <span className="sidebar__dot" aria-hidden="true" />
            <span>
              {paused
                ? "일시 정지 중"
                : settings?.placeMode === "classify"
                ? "자동 분류"
                : settings?.organizeMode === "manual"
                  ? "모아 두고 나중에 정리"
                  : settings?.placeMode === "auto"
                    ? "규칙대로 바로 정리"
                    : "바로 물어보기"}
            </span>
          </div>
          <div className="sidebar__status-text">
            {paused ? "새 파일은 묻지 않고 정리함에 모여요" : `지켜볼 폴더: ${watching}`}
          </div>
          <button
            type="button"
            className="sidebar__pause"
            onClick={togglePause}
            aria-label={paused ? "다시 시작" : "일시 정지"}
            title={paused ? "다시 시작" : "일시 정지(새 파일을 묻지 않고 정리함에 모아요)"}
          >
            {paused ? <Play size={14} aria-hidden="true" /> : <Pause size={14} aria-hidden="true" />}
            <span className="sidebar__pause-text">{paused ? "다시 시작" : "일시 정지"}</span>
          </button>
        </div>
      </nav>
      <main className="main-window__content">
        <UpdateBanner />
        {page === "inbox" ? <InboxPage /> : page === "rules" ? <RulesPage /> : <SettingsPage />}
      </main>
    </div>
  );
}
