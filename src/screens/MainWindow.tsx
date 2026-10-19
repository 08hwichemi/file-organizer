import { History, Inbox, ListChecks, Pause, PencilLine, Play, Search, Settings as SettingsIcon } from "lucide-react";
import { useEffect, useState } from "react";
import FolderIcon from "../components/FolderIcon";
import UpdateBanner from "../components/UpdateBanner";
import { getStatus, listInbox, onBackendEvent, setPaused } from "../lib/backend";
import { friendlyName } from "../lib/paths";
import { useAppStore, type Page } from "../store/useAppStore";
import { BUILD_LABEL } from "../lib/build";
import { startUpdateChecks } from "../store/useUpdateStore";
import InboxPage from "./InboxPage";
import LogPage from "./LogPage";
import RenamePage from "./RenamePage";
import RulesPage from "./RulesPage";
import SearchPage from "./SearchPage";
import SettingsPage from "./SettingsPage";
import "./MainWindow.css";

const MENU: { page: Page; label: string; Icon: typeof Inbox }[] = [
  { page: "inbox", label: "정리함", Icon: Inbox },
  { page: "log", label: "정리 기록", Icon: History },
  { page: "search", label: "파일 검색", Icon: Search },
  { page: "rename", label: "이름 바꾸기", Icon: PencilLine },
  { page: "rules", label: "정리 규칙", Icon: ListChecks },
  { page: "settings", label: "설정", Icon: SettingsIcon },
];

// 메인 창(SPEC 3-2): 왼쪽 메뉴 + 오른쪽 내용. 메뉴는 기능이 생기는 단계마다 늘어난다.
export default function MainWindow() {
  const page = useAppStore((s) => s.page);
  const goTo = useAppStore((s) => s.goTo);
  const openSearch = useAppStore((s) => s.openSearch);
  const settings = useAppStore((s) => s.settings);
  const known = useAppStore((s) => s.setupInfo);

  const [inboxCount, setInboxCount] = useState(0);
  const [paused, setPausedState] = useState(false);

  useEffect(() => startUpdateChecks(), []);

  // 트레이 메뉴 "정리함" → 그 화면으로
  useEffect(
    () =>
      onBackendEvent("navigate", (page) => {
        if (page === "inbox" || page === "search" || page === "rules" || page === "settings") goTo(page);
      }),
    [goTo],
  );

  // 단축키·트레이 "파일 검색" → 검색 화면 + 입력 칸에 초점
  useEffect(() => onBackendEvent("search-opened", openSearch), [openSearch]);

  // 팝업에서 바꾼 설정("이 폴더는 묻지 않기")을 다시 읽는다
  const refreshSettings = useAppStore((s) => s.refreshSettings);
  useEffect(
    () => onBackendEvent("settings-changed", () => void refreshSettings().catch(() => {})),
    [refreshSettings],
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

  // 새 파일을 찾는 곳: 내 PC 어디든 / 지켜볼 폴더 이름
  const watching =
    settings?.newFileScope === "anywhere"
      ? "새 파일: 내 PC 어디든"
      : `지켜볼 폴더: ${settings?.watchFolders.map((w) => friendlyName(w, known)).join(" · ") ?? ""}`;

  return (
    <div className="main-window">
      <nav className="sidebar" aria-label="메뉴">
        <div className="sidebar__brand">
          <span className="sidebar__logo" aria-hidden="true">
            <FolderIcon size={18} />
          </span>
          <span className="sidebar__brand-text">AutoFolder</span>
        </div>
        {/* 시험판 표시는 이름 아래 노란 줄(이름 옆에 두면 "AutoFol / der"처럼 갈라졌다) */}
        {BUILD_LABEL && (
          <div className="sidebar__build" title="PR에서 만든 시험판이에요. 정식 배포판은 이 표시가 없어요.">
            {BUILD_LABEL}
          </div>
        )}
        {MENU.map(({ page: p, label, Icon }) => (
          <button
            key={p}
            type="button"
            className={`sidebar__item${page === p ? " sidebar__item--on" : ""}`}
            title={p === "search" && settings?.searchShortcut ? `${label} (${settings.searchShortcut})` : label}
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
          title={paused ? "일시 정지 중: 새 파일은 팝업 없이 정리함에 모여요" : watching}
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
            {paused ? "새 파일은 묻지 않고 정리함에 모여요" : watching}
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
        {page === "inbox" ? (
          <InboxPage />
        ) : page === "log" ? (
          <LogPage />
        ) : page === "search" ? (
          <SearchPage />
        ) : page === "rename" ? (
          <RenamePage />
        ) : page === "rules" ? (
          <RulesPage />
        ) : (
          <SettingsPage />
        )}
      </main>
    </div>
  );
}
