// 업데이트(SPEC 7장).
// - 자동 업데이트(설정 기본 켬): 확인·받기·설치는 Rust(autoupdate.rs)가 한다. 켜고 3분 안에 받으면 바로,
//   그 뒤에는 창이 닫혀 있을 때 설치하고 다시 켠다(사용자 요청: 누르지 않아도 업데이트되게).
//   화면은 받아 둔 버전을 보여 주고([지금 업데이트]로 바로 설치), 업데이트 뒤 "…버전으로 업데이트했어요"를 알린다.
// - 끄면 예전처럼 화면이 켤 때와 4시간마다 확인 → "새 버전이 나왔어요 · 지금 업데이트" → 설치 후 다시 시작.
// 업데이트 파일은 공개 저장소(08hwichemi/file-organizer-releases)의 Releases에서 받는다.
// 이 확인은 프로그램 버전만 묻고, 파일 이름·경로 같은 사용자 정보는 보내지 않는다.
import { create } from "zustand";
import {
  installReadyUpdate,
  onBackendEvent,
  prepareUpdate,
  readyUpdate,
  takeUpdateNotice,
  usingMocks,
} from "../lib/backend";
import { useAppStore } from "./useAppStore";

/**
 * idle·checking·latest·error = 확인 상태 / available = 새 버전 있음(직접 누르기) /
 * ready = 자동 업데이트가 받아 둠(창을 닫으면 설치) / downloading = [지금 업데이트]로 받는 중 /
 * installing = 설치 중(곧 꺼졌다 켜짐)
 */
type Status = "idle" | "checking" | "latest" | "available" | "ready" | "downloading" | "installing" | "error";

interface PendingUpdate {
  version: string;
  /** 받아서 설치하고 다시 켠다(다시 켜질 때 창을 보인다) */
  install: (onProgress: (percent: number | null) => void) => Promise<void>;
}

interface UpdateState {
  status: Status;
  version: string | null;
  progress: number | null;
  error: string | null;
  update: PendingUpdate | null;
  /** 업데이트 뒤 다시 켜졌으면 그 버전("0.1.35 버전으로 업데이트했어요", 닫으면 null) */
  updatedTo: string | null;
  /** quiet = 자동 확인(실패해도 알리지 않음) */
  check: (quiet?: boolean) => Promise<void>;
  /** [지금 업데이트] */
  install: () => Promise<void>;
  dismissUpdated: () => void;
}

async function findUpdate(): Promise<PendingUpdate | null> {
  if (usingMocks) {
    // 브라우저 미리보기: 주소 뒤 ?update=1 이면 새 버전이 있는 것처럼
    if (new URLSearchParams(window.location.search).get("update") !== "1") return null;
    return {
      version: "0.1.35",
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
      // 설치 프로그램은 처음 켜질 때의 인자(--autostart)로 다시 켠다 → 창을 보이게 표시를 남긴다(relaunch.rs).
      // 표시를 못 남겨도 업데이트는 한다(창은 트레이에서 열면 됨).
      await prepareUpdate(found.version, false).catch(() => undefined);
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
    if (["checking", "ready", "downloading", "installing"].includes(status)) return;
    set({ status: "checking", error: null });
    try {
      // 자동 업데이트가 이미 받아 두었으면 그것을 보여 준다
      const ready = await readyUpdate().catch(() => null);
      if (ready) {
        set({ status: "ready", version: ready, update: null });
        return;
      }
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
    const { status, update } = get();
    if (status === "ready") {
      set({ status: "installing", error: null });
      try {
        await installReadyUpdate();
      } catch {
        set({ status: "ready", error: "업데이트를 설치하지 못했어요. 잠시 뒤 다시 눌러 주세요." });
      }
      return;
    }
    if (!update) return;
    set({ status: "downloading", progress: 0, error: null });
    try {
      await update.install((progress) => set({ progress }));
    } catch {
      set({ status: "available", progress: null, error: "업데이트를 설치하지 못했어요. 잠시 뒤 다시 눌러 주세요." });
    }
  },

  dismissUpdated() {
    set({ updatedTo: null });
  },
}));

const CHECK_EVERY = 4 * 60 * 60 * 1000;

/**
 * 메인 창에서 한 번 부른다. 자동 업데이트면 Rust가 받아 둔 버전을 보여 주고(받으면 알림이 옴),
 * 아니면 지금 확인하고 그 뒤 4시간마다.
 */
export function startUpdateChecks(): () => void {
  void takeUpdateNotice()
    .then((v) => v && useUpdateStore.setState({ updatedTo: v }))
    .catch(() => {});
  const showReady = (v: unknown) => {
    if (typeof v === "string" && v) useUpdateStore.setState({ status: "ready", version: v, update: null });
  };
  const stopReady = onBackendEvent("update-ready", showReady);
  void readyUpdate().then(showReady).catch(() => {});
  const checkIfManual = () => {
    if (!autoUpdate()) void useUpdateStore.getState().check(true);
  };
  checkIfManual();
  const timer = window.setInterval(checkIfManual, CHECK_EVERY);
  return () => {
    stopReady();
    window.clearInterval(timer);
  };
}
