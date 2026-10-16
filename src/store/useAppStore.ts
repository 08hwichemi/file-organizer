import { create } from "zustand";
import * as backend from "../lib/backend";
import type { OrganizeMode, PlaceMode, Settings, SetupInfo, SetupRequest } from "../lib/types";

type Status = "loading" | "setup" | "ready" | "error";
export type Page = "inbox" | "search" | "rules" | "settings";

interface AppState {
  status: Status;
  settings: Settings | null;
  /** 문서·바탕화면·다운로드 위치와 예시 폴더 목록 */
  setupInfo: SetupInfo | null;
  loadError: string | null;
  page: Page;
  /** 파일 검색 입력 칸에 초점을 줄 때마다 1씩 늘린다(단축키·트레이로 열었을 때) */
  searchFocus: number;

  load: () => Promise<void>;
  finishSetup: (request: SetupRequest) => Promise<void>;
  setOrganizeMode: (mode: OrganizeMode) => Promise<void>;
  setWatchFolders: (folders: string[]) => Promise<void>;
  setPlaceMode: (mode: PlaceMode) => Promise<void>;
  setTermMonths: (first: number, second: number) => Promise<void>;
  setAutostart: (enabled: boolean) => Promise<void>;
  setWaitSeconds: (seconds: number) => Promise<void>;
  setPinnedFavorites: (pins: (string | null)[]) => Promise<void>;
  setRootFolder: (path: string) => Promise<void>;
  setSearchShortcut: (shortcut: string) => Promise<void>;
  setSearchWholePc: (enabled: boolean) => Promise<void>;
  goTo: (page: Page) => void;
  /** 파일 검색 화면으로 가서 입력 칸에 초점 */
  openSearch: () => void;
}

export const useAppStore = create<AppState>((set) => ({
  status: "loading",
  settings: null,
  setupInfo: null,
  loadError: null,
  page: "inbox",
  searchFocus: 0,

  async load() {
    // 프로그램이 막 켜지는 중이면 잠깐 실패할 수 있어 몇 번 다시 시도한다.
    let lastError: unknown = null;
    for (let attempt = 0; attempt < 5; attempt++) {
      try {
        const [settings, setupInfo] = await Promise.all([backend.getSettings(), backend.getSetupInfo()]);
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

  async setOrganizeMode(mode) {
    set({ settings: await backend.setOrganizeMode(mode) });
  },

  async setWatchFolders(folders) {
    set({ settings: await backend.setWatchFolders(folders) });
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

  async setSearchWholePc(enabled) {
    set({ settings: await backend.setSearchWholePc(enabled) });
  },

  goTo(page) {
    set({ page });
  },

  openSearch() {
    set((s) => ({ page: "search", searchFocus: s.searchFocus + 1 }));
  },
}));
