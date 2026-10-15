// 화면은 파일을 직접 만지지 않고 Rust 명령(invoke)을 부른다(SPEC 8장).
// 브라우저(npm run dev)에서 화면만 볼 때는 Tauri가 없으므로 src/mocks/의 가짜 데이터를 쓴다.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { mockBackend } from "../mocks/backend";
import type {
  AppInfo,
  FolderEntry,
  OrganizeMode,
  Settings,
  SetupInfo,
  SetupRequest,
} from "./types";

export const usingMocks = !isTauri();

export function getAppInfo(): Promise<AppInfo> {
  if (usingMocks) return mockBackend.appInfo();
  return invoke("app_info");
}

export function getSetupInfo(): Promise<SetupInfo> {
  if (usingMocks) return mockBackend.setupInfo();
  return invoke("setup_info");
}

export function getSettings(): Promise<Settings> {
  if (usingMocks) return mockBackend.getSettings();
  return invoke("get_settings");
}

export function completeSetup(request: SetupRequest): Promise<Settings> {
  if (usingMocks) return mockBackend.completeSetup(request);
  return invoke("complete_setup", { request });
}

export function setOrganizeMode(mode: OrganizeMode): Promise<Settings> {
  if (usingMocks) return mockBackend.setOrganizeMode(mode);
  return invoke("set_organize_mode", { mode });
}

export function setWatchFolders(folders: string[]): Promise<Settings> {
  if (usingMocks) return mockBackend.setWatchFolders(folders);
  return invoke("set_watch_folders", { folders });
}

export function listFolders(path: string): Promise<FolderEntry[]> {
  if (usingMocks) return mockBackend.listFolders(path);
  return invoke("list_folders", { path });
}

/** 폴더 고르기 창. 취소하면 null. */
export async function pickFolder(title: string): Promise<string | null> {
  if (usingMocks) return mockBackend.pickFolder();
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ directory: true, multiple: false, title });
  return typeof picked === "string" ? picked : null;
}

/** Rust가 돌려준 오류 문구(문자열)를 화면에 보여 줄 문장으로. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "알 수 없는 오류가 생겼어요.";
}
