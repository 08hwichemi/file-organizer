import { create } from "zustand";
import * as backend from "../lib/backend";
import type { OrganizeMode, Settings, SetupInfo, SetupRequest } from "../lib/types";

type Status = "loading" | "setup" | "ready" | "error";
export type Page = "inbox" | "settings";

interface AppState {
  status: Status;
  settings: Settings | null;
  /** 문서·바탕화면·다운로드 위치와 예시 폴더 목록 */
  setupInfo: SetupInfo | null;
  loadError: string | null;
  page: Page;

  load: () => Promise<void>;
  finishSetup: (request: SetupRequest) => Promise<void>;
  setOrganizeMode: (mode: OrganizeMode) => Promise<void>;
  setWatchFolders: (folders: string[]) => Promise<void>;
  goTo: (page: Page) => void;
}

export const useAppStore = create<AppState>((set) => ({
  status: "loading",
  settings: null,
  setupInfo: null,
  loadError: null,
  page: "inbox",

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

  // 아래 세 함수는 실패하면 오류를 그대로 던진다. 부른 화면이 문구를 보여 준다.
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

  goTo(page) {
    set({ page });
  },
}));
