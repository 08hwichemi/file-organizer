// 화면은 파일을 직접 만지지 않고 Rust 명령(invoke)을 부른다(SPEC 8장).
// 브라우저(npm run dev)에서 화면만 볼 때는 Tauri가 없으므로 src/mocks/의 가짜 데이터를 쓴다.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { mockBackend, mockEvents } from "../mocks/backend";
import type {
  AppInfo,
  ClassifyPlan,
  DestSegment,
  FolderEntry,
  FolderMatch,
  InboxItem,
  OrganizeMode,
  PendingFile,
  PlaceMode,
  PopupState,
  PutOutcome,
  PutResult,
  Rule,
  RuleInput,
  RuleView,
  Settings,
  SetupInfo,
  SetupRequest,
  TrashResult,
  UndoManyResult,
  WordGroup,
} from "./types";

export const usingMocks = !isTauri();

/** 이 화면이 새 파일 팝업 창인지(브라우저 미리보기에서는 주소 뒤 ?window=popup). */
export async function isPopupWindow(): Promise<boolean> {
  if (usingMocks) return new URLSearchParams(window.location.search).get("window") === "popup";
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow().label === "popup";
}

/** Rust가 보내는 알림: popup-changed(팝업 차례 바뀜), inbox-changed(정리함 바뀜) */
export type BackendEvent = "popup-changed" | "inbox-changed";

/** 알림을 듣는다. 돌려준 함수를 부르면 그만 듣는다. */
export function onBackendEvent(name: BackendEvent, handler: () => void): () => void {
  if (usingMocks) return mockEvents.on(name, handler);
  let stop: (() => void) | null = null;
  let cancelled = false;
  void import("@tauri-apps/api/event").then(({ listen }) =>
    listen(name, () => handler()).then((unlisten) => {
      if (cancelled) unlisten();
      else stop = unlisten;
    }),
  );
  return () => {
    cancelled = true;
    stop?.();
  };
}

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

export function setPlaceMode(mode: PlaceMode): Promise<Settings> {
  if (usingMocks) return mockBackend.setPlaceMode(mode);
  return invoke("set_place_mode", { mode });
}

/** 1학기·2학기가 시작하는 달 */
export function setTermMonths(first: number, second: number): Promise<Settings> {
  if (usingMocks) return mockBackend.setTermMonths(first, second);
  return invoke("set_term_months", { first, second });
}

export function listFolders(path: string): Promise<FolderEntry[]> {
  if (usingMocks) return mockBackend.listFolders(path);
  return invoke("list_folders", { path });
}

export function createFolder(parent: string, name: string): Promise<FolderEntry> {
  if (usingMocks) return mockBackend.createFolder(parent, name);
  return invoke("create_folder", { parent, name });
}

export function searchFolders(query: string): Promise<FolderMatch[]> {
  if (usingMocks) return mockBackend.searchFolders(query);
  return invoke("search_folders", { query });
}

export function getRecentDestinations(): Promise<string[]> {
  if (usingMocks) return mockBackend.recentDestinations();
  return invoke("recent_destinations");
}

export function getPopupState(): Promise<PopupState> {
  if (usingMocks) return mockBackend.popupState();
  return invoke("popup_state");
}

export function putFile(path: string, destDir: string, newName: string | null): Promise<PutResult> {
  if (usingMocks) return mockBackend.putFile(path, destDir, newName);
  return invoke("put_file", { path, destDir, newName });
}

export function undoMove(moveId: number, requeue: boolean): Promise<PendingFile> {
  if (usingMocks) return mockBackend.undoMove(moveId, requeue);
  return invoke("undo_move", { moveId, requeue });
}

export function deferFile(path: string): Promise<void> {
  if (usingMocks) return mockBackend.deferFile(path);
  return invoke("defer_file", { path });
}

export function hidePopup(): Promise<void> {
  if (usingMocks) return mockBackend.hidePopup();
  return invoke("hide_popup");
}

export function putFiles(paths: string[], destDir: string): Promise<PutOutcome[]> {
  if (usingMocks) return mockBackend.putFiles(paths, destDir);
  return invoke("put_files", { paths, destDir });
}

export function undoMoves(moveIds: number[]): Promise<UndoManyResult> {
  if (usingMocks) return mockBackend.undoMoves(moveIds);
  return invoke("undo_moves", { moveIds });
}

/** 지켜볼 폴더에 지금 있는 파일을 정리함으로 가져온다. 새로 들어온 수. */
export function importExisting(): Promise<number> {
  if (usingMocks) return mockBackend.importExisting();
  return invoke("import_existing");
}

/** 자주 쓰는 폴더(숫자 키 1~6) */
export function getFavoriteFolders(): Promise<string[]> {
  if (usingMocks) return mockBackend.favoriteFolders();
  return invoke("favorite_folders");
}

export function renameFolder(path: string, newName: string): Promise<FolderEntry> {
  if (usingMocks) return mockBackend.renameFolder(path, newName);
  return invoke("rename_folder", { path, newName });
}

export function moveFolder(path: string, destParent: string): Promise<FolderEntry> {
  if (usingMocks) return mockBackend.moveFolder(path, destParent);
  return invoke("move_folder", { path, destParent });
}

/** 폴더를 휴지통으로. 되돌리기용 번호. */
export function trashFolder(path: string): Promise<number> {
  if (usingMocks) return mockBackend.trashFolder(path);
  return invoke("trash_folder", { path });
}

/** 파일들을 휴지통으로 */
export function trashFiles(paths: string[]): Promise<TrashResult> {
  if (usingMocks) return mockBackend.trashFiles(paths);
  return invoke("trash_files", { paths });
}

/** 휴지통으로 보낸 것을 원래 자리로 */
export function restoreDeleted(deletionIds: number[]): Promise<UndoManyResult> {
  if (usingMocks) return mockBackend.restoreDeleted(deletionIds);
  return invoke("restore_deleted", { deletionIds });
}

export function listInbox(): Promise<InboxItem[]> {
  if (usingMocks) return mockBackend.listInbox();
  return invoke("list_inbox");
}

/** "바로 정리" 알림 닫기 */
export function dismissAuto(moveIds: number[]): Promise<void> {
  if (usingMocks) return mockBackend.dismissAuto(moveIds);
  return invoke("dismiss_auto", { moveIds });
}

/** 추천 위치에 넣기(정리함) */
export function putSuggested(paths: string[]): Promise<PutOutcome[]> {
  if (usingMocks) return mockBackend.putSuggested(paths);
  return invoke("put_suggested", { paths });
}

/** 자동 분류 미리보기 */
export function classifyPreview(paths: string[]): Promise<ClassifyPlan[]> {
  if (usingMocks) return mockBackend.classifyPreview(paths);
  return invoke("classify_preview", { paths });
}

/** 자동 분류 실행(나누지 못한 파일은 정리함에 남음) */
export function classifyFiles(paths: string[]): Promise<PutOutcome[]> {
  if (usingMocks) return mockBackend.classifyFiles(paths);
  return invoke("classify_files", { paths });
}

export function getAutoWords(): Promise<WordGroup[]> {
  if (usingMocks) return mockBackend.getAutoWords();
  return invoke("get_auto_words");
}

/** 자동 분류 단어 저장. null이면 기본 단어로 되돌린다. */
export function setAutoWords(groups: WordGroup[] | null): Promise<WordGroup[]> {
  if (usingMocks) return mockBackend.setAutoWords(groups);
  return invoke("set_auto_words", { groups });
}

export function listRules(): Promise<RuleView[]> {
  if (usingMocks) return mockBackend.listRules();
  return invoke("list_rules");
}

/** 폴더를 정리 폴더 아래 단계별로(나눌 수 있는 연도·학기·학급 폴더 표시) */
export function describeDest(destDir: string): Promise<DestSegment[]> {
  if (usingMocks) return mockBackend.describeDest(destDir);
  return invoke("describe_dest", { destDir });
}

export function createRule(input: RuleInput): Promise<Rule> {
  if (usingMocks) return mockBackend.createRule(input);
  return invoke("create_rule", { input });
}

export function updateRule(id: number, input: RuleInput): Promise<Rule> {
  if (usingMocks) return mockBackend.updateRule(id, input);
  return invoke("update_rule", { id, input });
}

export function deleteRule(id: number): Promise<void> {
  if (usingMocks) return mockBackend.deleteRule(id);
  return invoke("delete_rule", { id });
}

export function restoreRule(id: number): Promise<void> {
  if (usingMocks) return mockBackend.restoreRule(id);
  return invoke("restore_rule", { id });
}

/** 규칙 순서 한 칸 위·아래로 */
export function moveRule(id: number, up: boolean): Promise<boolean> {
  if (usingMocks) return mockBackend.moveRule(id, up);
  return invoke("move_rule", { id, up });
}

/** "규칙을 바꿀까요?" 답: destDir가 있으면 그 폴더로 바꾸고, null이면 그대로 둔다 */
export function answerRuleChange(id: number, destDir: string | null): Promise<void> {
  if (usingMocks) return mockBackend.answerRuleChange(id, destDir);
  return invoke("answer_rule_change", { id, destDir });
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
