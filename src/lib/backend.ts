// 화면은 파일을 직접 만지지 않고 Rust 명령(invoke)을 부른다(SPEC 8장).
// 브라우저(npm run dev)에서 화면만 볼 때는 Tauri가 없으므로 src/mocks/의 가짜 데이터를 쓴다.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { mockAppInfo } from "../mocks/appInfo";

export interface AppInfo {
  name: string;
  version: string;
  os: string;
}

export function getAppInfo(): Promise<AppInfo> {
  if (!isTauri()) return Promise.resolve(mockAppInfo);
  return invoke<AppInfo>("app_info");
}
