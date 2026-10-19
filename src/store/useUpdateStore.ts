// 업데이트(SPEC 7장): 켤 때 + 4시간마다 확인.
// - 자동 업데이트(설정 기본 켬): 새 버전을 뒤에서 받아 두었다가, 프로그램을 쓰지 않을 때(창이 닫혀 있고 물어볼 파일이
//   없을 때) 알아서 설치하고 트레이로 조용히 다시 켠다(사용자 요청: 누르지 않아도 업데이트되게).
//   창을 열고 있으면 "받아 두었어요 · 창을 닫으면 설치해요 [지금 업데이트]".
// - 끄면 예전처럼 "새 버전이 있어요 · 지금 업데이트" → 설치 후 다시 시작(메인 창이 보이게).
// 업데이트 파일은 공개 저장소(08hwichemi/file-organizer-releases)의 Releases에서 받는다.
// 이 확인은 프로그램 버전만 묻고, 파일 이름·경로 같은 사용자 정보는 보내지 않는다.
import { create } from "zustand";
import { prepareUpdate, takeUpdateNotice, updateIdle, usingMocks } from "../lib/backend";
import { useAppStore } from "./useAppStore";

/**
 * idle·checking·latest·error = 확인 상태 / available = 새 버전 있음(직접 누르기) /
 * fetching = 뒤에서 받는 중(자동) / ready = 받아 둠, 쓰지 않을 때 설치(자동) /
 * downloading = [지금 업데이트]로 받는 중 / installing = 설치 중(곧 꺼졌다 켜짐)
 */
type Status = "idle" | "checking" | "latest" | "available" | "fetching" | "ready" | "downloading" | "installing" | "error";

interface PendingUpdate {
  version: string;
  /** 받아 두기(이미 받았으면 바로 끝남) */
  download: (onProgress: (percent: number | null) => void) => Promise<void>;
  /** 받아 둔 것을 설치하고 다시 켠다. `quiet`(자동)면 다시 켜질 때 창을 띄우지 않는다 */
  install: (quiet: boolean) => Promise<void>;
}

interface UpdateState {
  status: Status;
  version: string | null;
  progress: number | null;
  error: string | null;
  update: PendingUpdate | null;
  /** 업데이트 뒤 다시 켜졌으면 그 버전("0.1.35로 업데이트했어요", 닫으면 null) */
  updatedTo: string | null;
  /** quiet = 자동 확인(실패해도 알리지 않음) */
  check: (quiet?: boolean) => Promise<void>;
  /** [지금 업데이트] */
  install: () => Promise<void>;
  /** 자동 업데이트: 받아 두었고 지금 쓰지 않으면 설치 */
  installWhenIdle: () => Promise<void>;
  dismissUpdated: () => void;
}

async function findUpdate(): Promise<PendingUpdate | null> {
  if (usingMocks) {
    // 브라우저 미리보기: 주소 뒤 ?update=1 이면 새 버전이 있는 것처럼
    if (new URLSearchParams(window.location.search).get("update") !== "1") return null;
    return {
      version: "0.1.35",
      download: async (onProgress) => {
        for (let p = 0; p <= 100; p += 20) {
          onProgress(p);
          await new Promise((r) => setTimeout(r, 150));
        }
      },
      install: async () => {},
    };
  }
  const { check } = await import("@tauri-apps/plugin-updater");
  const found = await check();
  if (!found) return null;
  let downloaded = false;
  return {
    version: found.version,
    download: async (onProgress) => {
      if (downloaded) return;
      let total = 0;
      let received = 0;
      await found.download((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        if (event.event === "Progress") {
          received += event.data.chunkLength;
          onProgress(total > 0 ? Math.min(100, Math.round((received / total) * 100)) : null);
        }
      });
      downloaded = true;
    },
    install: async (quiet) => {
      // 설치 프로그램은 처음 켜질 때의 인자(--autostart)로 다시 켠다 → 창을 보일지 표시를 남긴다(relaunch.rs).
      // 표시를 못 남겨도 업데이트는 한다(창은 트레이에서 열면 됨).
      await prepareUpdate(found.version, quiet).catch(() => undefined);
      await found.install();
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    },
  };
}

const autoUpdate = () => useAppStore.getState().settings?.autoUpdate ?? true;

export const useUpdateStore = create<UpdateState>((set, get) => ({
  status: "idle",
  version: null,
  progress: null,
  error: null,
  update: null,
  updatedTo: null,

  async check(quiet = false) {
    const { status } = get();
    if (["checking", "fetching", "ready", "downloading", "installing"].includes(status)) return;
    set({ status: "checking", error: null });
    let update: PendingUpdate | null;
    try {
      update = await findUpdate();
    } catch {
      // 인터넷이 안 되는 등. 자동 확인일 때는 조용히 넘어간다.
      set({ status: quiet ? "idle" : "error", error: quiet ? null : "업데이트를 확인하지 못했어요. 인터넷 연결을 확인해 주세요." });
      return;
    }
    if (!update) {
      set({ status: "latest", update: null, version: null });
      return;
    }
    if (!autoUpdate()) {
      set({ status: "available", update, version: update.version });
      return;
    }
    // 자동: 뒤에서 받아 두고, 쓰지 않을 때 설치
    set({ status: "fetching", update, version: update.version });
    try {
      await update.download(() => {});
      set({ status: "ready" });
      await get().installWhenIdle();
    } catch {
      set({ status: "available" }); // 못 받으면 예전처럼 직접 누르게
    }
  },

  async install() {
    const { update } = get();
    if (!update) return;
    set({ status: "downloading", progress: 0, error: null });
    try {
      await update.download((progress) => set({ progress }));
      set({ status: "installing" });
      await update.install(false);
    } catch {
      set({ status: "available", progress: null, error: "업데이트를 설치하지 못했어요. 잠시 뒤 다시 눌러 주세요." });
    }
  },

  async installWhenIdle() {
    const { status, update } = get();
    if (status !== "ready" || !update || !autoUpdate()) return;
    if (!(await updateIdle().catch(() => false))) return;
    set({ status: "installing" });
    try {
      await update.install(true);
    } catch {
      set({ status: "ready" }); // 다음에 다시
    }
  },

  dismissUpdated() {
    set({ updatedTo: null });
  },
}));

const CHECK_EVERY = 4 * 60 * 60 * 1000;
/** 받아 둔 업데이트를 설치해도 되는지 보는 간격 */
const IDLE_EVERY = 60 * 1000;

/** 메인 창에서 한 번 부른다: 지금 확인하고, 그 뒤 4시간마다. 받아 둔 업데이트는 1분마다 쓰지 않는지 본다. */
export function startUpdateChecks(): () => void {
  void takeUpdateNotice()
    .then((v) => v && useUpdateStore.setState({ updatedTo: v }))
    .catch(() => {});
  void useUpdateStore.getState().check(true);
  const check = window.setInterval(() => void useUpdateStore.getState().check(true), CHECK_EVERY);
  const idle = window.setInterval(() => void useUpdateStore.getState().installWhenIdle(), IDLE_EVERY);
  return () => {
    window.clearInterval(check);
    window.clearInterval(idle);
  };
}
