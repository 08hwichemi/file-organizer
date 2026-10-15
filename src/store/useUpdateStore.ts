// 자동 업데이트(SPEC 7장): 시작할 때 + 하루 한 번 확인 → "새 버전이 있어요 · 지금 업데이트" → 설치 후 다시 시작.
// 업데이트 파일은 공개 저장소(08hwichemi/file-organizer-releases)의 Releases에서 받는다.
// 이 확인은 프로그램 버전만 묻고, 파일 이름·경로 같은 사용자 정보는 보내지 않는다.
import { create } from "zustand";
import { usingMocks } from "../lib/backend";

type Status = "idle" | "checking" | "latest" | "available" | "downloading" | "error";

interface PendingUpdate {
  version: string;
  install: (onProgress: (percent: number | null) => void) => Promise<void>;
}

interface UpdateState {
  status: Status;
  version: string | null;
  progress: number | null;
  error: string | null;
  update: PendingUpdate | null;
  /** quiet = 자동 확인(실패해도 알리지 않음) */
  check: (quiet?: boolean) => Promise<void>;
  install: () => Promise<void>;
}

async function findUpdate(): Promise<PendingUpdate | null> {
  if (usingMocks) {
    // 브라우저 미리보기: 주소 뒤 ?update=1 이면 새 버전이 있는 것처럼
    if (new URLSearchParams(window.location.search).get("update") !== "1") return null;
    return {
      version: "0.1.9",
      install: async (onProgress) => {
        for (let p = 0; p <= 100; p += 20) {
          onProgress(p);
          await new Promise((r) => setTimeout(r, 150));
        }
      },
    };
  }
  const { check } = await import("@tauri-apps/plugin-updater");
  const found = await check();
  if (!found) return null;
  return {
    version: found.version,
    install: async (onProgress) => {
      let total = 0;
      let received = 0;
      await found.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        if (event.event === "Progress") {
          received += event.data.chunkLength;
          onProgress(total > 0 ? Math.min(100, Math.round((received / total) * 100)) : null);
        }
      });
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    },
  };
}

export const useUpdateStore = create<UpdateState>((set, get) => ({
  status: "idle",
  version: null,
  progress: null,
  error: null,
  update: null,

  async check(quiet = false) {
    const { status } = get();
    if (status === "checking" || status === "downloading") return;
    set({ status: "checking", error: null });
    try {
      const update = await findUpdate();
      set(
        update
          ? { status: "available", update, version: update.version }
          : { status: "latest", update: null, version: null },
      );
    } catch {
      // 인터넷이 안 되는 등. 자동 확인일 때는 조용히 넘어간다.
      set({ status: quiet ? "idle" : "error", error: quiet ? null : "업데이트를 확인하지 못했어요. 인터넷 연결을 확인해 주세요." });
    }
  },

  async install() {
    const { update } = get();
    if (!update) return;
    set({ status: "downloading", progress: 0, error: null });
    try {
      await update.install((progress) => set({ progress }));
    } catch {
      set({ status: "available", progress: null, error: "업데이트를 설치하지 못했어요. 잠시 뒤 다시 눌러 주세요." });
    }
  },
}));

const DAY = 24 * 60 * 60 * 1000;

/** 메인 창에서 한 번 부른다: 지금 확인하고, 그 뒤 하루에 한 번. */
export function startUpdateChecks(): () => void {
  void useUpdateStore.getState().check(true);
  const timer = window.setInterval(() => void useUpdateStore.getState().check(true), DAY);
  return () => window.clearInterval(timer);
}
