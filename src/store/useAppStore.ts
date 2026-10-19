import { create } from "zustand";
import * as backend from "../lib/backend";
import { setKnownFolders } from "../lib/paths";
import type { NewFileScope, OrganizeMode, PlaceMode, Settings, SetupInfo, SetupRequest, UserKind } from "../lib/types";

/** redo = 설정에서 "처음 설정 다시 하기"를 연 상태(취소하면 ready로) */
type Status = "loading" | "setup" | "redo" | "ready" | "error";
export type Page = "inbox" | "log" | "search" | "rename" | "rules" | "settings";
/** 설정 화면의 탭: 정리 방식 · 폴더 · 파일 검색 · 프로그램 */
export type SettingsTab = "organize" | "folders" | "search" | "app";

interface AppState {
  status: Status;
  settings: Settings | null;
  /** 문서·바탕화면·다운로드 위치와 예시 폴더 목록 */
  setupInfo: SetupInfo | null;
  loadError: string | null;
  page: Page;
  /** 설정 화면에서 보고 있는 탭(다른 화면에 갔다 와도 그대로) */
  settingsTab: SettingsTab;
  /** 파일 검색 입력 칸에 초점을 줄 때마다 1씩 늘린다(단축키·트레이로 열었을 때) */
  searchFocus: number;

  load: () => Promise<void>;
  finishSetup: (request: SetupRequest) => Promise<void>;
  /** 처음 설정 화면을 지금 설정으로 채워 다시 연다 / 닫는다 */
  redoSetup: (open: boolean) => void;
  setOrganizeMode: (mode: OrganizeMode) => Promise<void>;
  setWatchFolders: (folders: string[]) => Promise<void>;
  /** 다른 창에서 바뀐 설정만 다시 읽는다(화면 상태는 그대로) */
  refreshSettings: () => Promise<void>;
  setNewFileScope: (scope: NewFileScope) => Promise<void>;
  setQuietFolders: (folders: string[]) => Promise<void>;
  setPlaceMode: (mode: PlaceMode) => Promise<void>;
  setTermMonths: (first: number, second: number) => Promise<void>;
  setAutostart: (enabled: boolean) => Promise<void>;
  setWaitSeconds: (seconds: number) => Promise<void>;
  setPinnedFavorites: (pins: (string | null)[]) => Promise<void>;
  setRootFolder: (path: string) => Promise<void>;
  setSearchShortcut: (shortcut: string) => Promise<void>;
  setSearchWholePc: (enabled: boolean) => Promise<void>;
  setUserKind: (kind: UserKind) => Promise<void>;
  goTo: (page: Page) => void;
  /** 설정 화면의 그 탭으로 */
  openSettings: (tab: SettingsTab) => void;
  setSettingsTab: (tab: SettingsTab) => void;
  /** 파일 검색 화면으로 가서 입력 칸에 초점 */
  openSearch: () => void;
}

export const useAppStore = create<AppState>((set) => ({
  status: "loading",
  settings: null,
  setupInfo: null,
  loadError: null,
  page: "inbox",
  settingsTab: "organize",
  searchFocus: 0,

  async load() {
    // 프로그램이 막 켜지는 중이면 잠깐 실패할 수 있어 몇 번 다시 시도한다.
    let lastError: unknown = null;
    for (let attempt = 0; attempt < 5; attempt++) {
      try {
        const [settings, setupInfo] = await Promise.all([backend.getSettings(), backend.getSetupInfo()]);
        setKnownFolders(setupInfo);
        set({ settings, setupInfo, status: settings.setupDone ? "ready" : "setup", loadError: null });
        return;
      } catch (error) {
        lastError = error;
        await new Promise((resolve) => setTimeout(resolve, 300));
      }
    }
    set({ status: "error", loadError: backend.errorMessage(lastError) });
  },

  // 아래 함수들은 실패하면 오류를 그대로 던진다. 부른 화면이 문구를 보여 준다.
  async finishSetup(request) {
    const settings = await backend.completeSetup(request);
    set({ settings, status: "ready", page: "inbox" });
  },

  redoSetup(open) {
    set({ status: open ? "redo" : "ready" });
  },

  async setOrganizeMode(mode) {
    set({ settings: await backend.setOrganizeMode(mode) });
  },

  async refreshSettings() {
    set({ settings: await backend.getSettings() });
  },

  async setWatchFolders(folders) {
    set({ settings: await backend.setWatchFolders(folders) });
  },

  async setNewFileScope(scope) {
    set({ settings: await backend.setNewFileScope(scope) });
  },

  async setQuietFolders(folders) {
    set({ settings: await backend.setQuietFolders(folders) });
  },

  async setPlaceMode(mode) {
    set({ settings: await backend.setPlaceMode(mode) });
  },

  async setTermMonths(first, second) {
    set({ settings: await backend.setTermMonths(first, second) });
  },

  async setAutostart(enabled) {
    set({ settings: await backend.setAutostart(enabled) });
  },

  async setWaitSeconds(seconds) {
    set({ settings: await backend.setWaitSeconds(seconds) });
  },

  async setPinnedFavorites(pins) {
    set({ settings: await backend.setPinnedFavorites(pins) });
  },

  async setRootFolder(path) {
    set({ settings: await backend.setRootFolder(path) });
  },

  async setSearchShortcut(shortcut) {
    set({ settings: await backend.setSearchShortcut(shortcut) });
  },

  async setUserKind(kind) {
    set({ settings: await backend.setUserKind(kind) });
  },

  async setSearchWholePc(enabled) {
    set({ settings: await backend.setSearchWholePc(enabled) });
  },

  goTo(page) {
    set({ page });
  },

  openSettings(tab) {
    set({ page: "settings", settingsTab: tab });
  },

  setSettingsTab(tab) {
    set({ settingsTab: tab });
  },

  openSearch() {
    set((s) => ({ page: "search", searchFocus: s.searchFocus + 1 }));
  },
}));
