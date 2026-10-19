// 화면은 파일을 직접 만지지 않고 Rust 명령(invoke)을 부른다(SPEC 8장).
// 브라우저(npm run dev)에서 화면만 볼 때는 Tauri가 없으므로 src/mocks/의 가짜 데이터를 쓴다.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { mockBackend, mockEvents } from "../mocks/backend";
import type {
  XlsxRow,
  LogEntry,
  AppInfo,
  BulkAction,
  NewFileScope,
  ClassifyPlan,
  DestSegment,
  FileEntry,
  FolderEntry,
  FolderFiles,
  FolderPreview,
  FolderMatch,
  InboxItem,
  OrganizeMode,
  PendingFile,
  PlaceMode,
  PopupState,
  PutOutcome,
  PutResult,
  RecentMove,
  RenamePreview,
  RenameRequest,
  SearchSort,
  RenameResult,
  SearchInfo,
  SearchResponse,
  Rule,
  RuleInput,
  RuleView,
  Settings,
  SetupInfo,
  Status,
  SetupRequest,
  TrashResult,
  UndoManyResult,
  UserKind,
  WordGroup,
} from "./types";

export const usingMocks = !isTauri();

/** 이 화면이 어느 창인지: main / popup(새 파일). 브라우저 미리보기에서는 주소 뒤 ?window=… */
export type WindowKind = "main" | "popup";

export async function windowKind(): Promise<WindowKind> {
  const label = usingMocks
    ? new URLSearchParams(window.location.search).get("window")
    : (await import("@tauri-apps/api/window")).getCurrentWindow().label;
  return label === "popup" ? label : "main";
}

/**
 * Rust가 보내는 알림: popup-changed(팝업 차례 바뀜), inbox-changed(정리함 바뀜),
 * status-changed(일시 정지 등), navigate(트레이에서 고른 화면, 값 = "inbox" 등),
 * search-opened(단축키·트레이로 파일 검색을 엶), search-indexed(파일 목록을 다 읽음),
 * folders-changed(정리 폴더 안의 폴더가 생기거나 바뀜: 프로그램에서 했거나 탐색기에서 했거나),
 * settings-changed(다른 창에서 설정을 바꿈: 팝업의 "이 폴더는 묻지 않기")
 */
export type BackendEvent =
  | "popup-changed"
  | "inbox-changed"
  | "status-changed"
  | "navigate"
  | "search-opened"
  | "search-indexed"
  | "folders-changed"
  | "settings-changed";

/** 알림을 듣는다. 돌려준 함수를 부르면 그만 듣는다. */
export function onBackendEvent(name: BackendEvent, handler: (payload: unknown) => void): () => void {
  if (usingMocks) return mockEvents.on(name, handler);
  let stop: (() => void) | null = null;
  let cancelled = false;
  void import("@tauri-apps/api/event").then(({ listen }) =>
    listen(name, (e) => handler(e.payload)).then((unlisten) => {
      if (cancelled) unlisten();
      else stop = unlisten;
    }),
  );
  return () => {
    cancelled = true;
    stop?.();
  };
}

/**
 * 폴더를 만들거나 바꾼 뒤, 열려 있는 모든 폴더 칸(다른 창 포함)이 바로 새로 읽게 알린다.
 * 탐색기에서 바꾼 것은 Rust 감시가 따로 알려 준다.
 */
function foldersChanged<T>(result: T): T {
  if (usingMocks) mockEvents.emit("folders-changed");
  else void import("@tauri-apps/api/event").then(({ emit }) => emit("folders-changed"));
  return result;
}

export function getAppInfo(): Promise<AppInfo> {
  if (usingMocks) return mockBackend.appInfo();
  return invoke("app_info");
}

/** 업데이트 설치 직전: 새 버전으로 다시 켜지면 메인 창을 보이게(자동 실행에서 업데이트해도). */
/** 업데이트 설치 직전: `quiet`(자동 업데이트)면 다시 켜질 때 창을 띄우지 않는다 */
export function prepareUpdate(version: string, quiet = false): Promise<void> {
  if (usingMocks) return Promise.resolve();
  return invoke("prepare_update", { version, quiet });
}

/** 자동 업데이트를 지금 설치해도 되는지(창이 닫혀 있고 물어볼 파일이 없을 때) */
export function updateIdle(): Promise<boolean> {
  if (usingMocks) return Promise.resolve(false);
  return invoke("update_idle");
}

/** 업데이트 뒤 다시 켜졌으면 그 버전(한 번만) */
export function takeUpdateNotice(): Promise<string | null> {
  if (usingMocks) return Promise.resolve(new URLSearchParams(window.location.search).get("updated") === "1" ? "0.1.35" : null);
  return invoke("take_update_notice");
}

/** 새 버전은 자동으로 설치 */
export function setAutoUpdate(enabled: boolean): Promise<Settings> {
  if (usingMocks) return mockBackend.setAutoUpdate(enabled);
  return invoke("set_auto_update", { enabled });
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

/** 쓰는 곳 바꾸기(학교/일반). 이미 만든 폴더는 그대로. */
export function setUserKind(kind: UserKind): Promise<Settings> {
  if (usingMocks) return mockBackend.setUserKind(kind);
  return invoke("set_user_kind", { kind });
}

export function setOrganizeMode(mode: OrganizeMode): Promise<Settings> {
  if (usingMocks) return mockBackend.setOrganizeMode(mode);
  return invoke("set_organize_mode", { mode });
}

export function setWatchFolders(folders: string[]): Promise<Settings> {
  if (usingMocks) return mockBackend.setWatchFolders(folders);
  return invoke("set_watch_folders", { folders });
}

/** 새 파일을 찾는 곳: 내 PC 어디든 / 지켜볼 폴더만 */
export function setNewFileScope(scope: NewFileScope): Promise<Settings> {
  if (usingMocks) return mockBackend.setNewFileScope(scope);
  return invoke("set_new_file_scope", { scope });
}

/** 묻지 않는 폴더(설정에서 더하기·빼기) */
export function setQuietFolders(folders: string[]): Promise<Settings> {
  if (usingMocks) return mockBackend.setQuietFolders(folders);
  return invoke("set_quiet_folders", { folders });
}

/** 팝업 ⋯ "이 폴더는 묻지 않기": 이 폴더에서 기다리던 파일도 더 묻지 않는다 */
export function quietFolder(dir: string): Promise<Settings> {
  if (usingMocks) return mockBackend.quietFolder(dir);
  return invoke("quiet_folder", { dir });
}

/** 한꺼번에 생긴 파일 알림 닫기 */
export function closeBulk(action: BulkAction): Promise<void> {
  if (usingMocks) return mockBackend.closeBulk(action);
  return invoke("close_bulk", { action });
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

export function getStatus(): Promise<Status> {
  if (usingMocks) return mockBackend.getStatus();
  return invoke("get_status");
}

/** 일시 정지 / 다시 시작 */
export function setPaused(paused: boolean): Promise<Status> {
  if (usingMocks) return mockBackend.setPaused(paused);
  return invoke("set_paused", { paused });
}

export function setAutostart(enabled: boolean): Promise<Settings> {
  if (usingMocks) return mockBackend.setAutostart(enabled);
  return invoke("set_autostart", { enabled });
}

export function setWaitSeconds(seconds: number): Promise<Settings> {
  if (usingMocks) return mockBackend.setWaitSeconds(seconds);
  return invoke("set_wait_seconds", { seconds });
}

export function setPinnedFavorites(pins: (string | null)[]): Promise<Settings> {
  if (usingMocks) return mockBackend.setPinnedFavorites(pins);
  return invoke("set_pinned_favorites", { pins });
}

/** 정리 폴더를 다른 폴더로(원래 폴더·파일은 그대로) */
export function setRootFolder(path: string): Promise<Settings> {
  if (usingMocks) return mockBackend.setRootFolder(path);
  return invoke("set_root_folder", { path });
}

/** 파일 검색(Everything이 켜져 있으면 Everything, 아니면 이 프로그램이 읽은 목록) */
export function searchFiles(query: string, sort: SearchSort = "best"): Promise<SearchResponse> {
  if (usingMocks) return mockBackend.searchFiles(query, sort);
  return invoke("search_files", { query, sort });
}

/** 검색 화면을 열 때: 파일 목록이 없거나 오래됐으면 뒤에서 읽기 시작 */
export function prepareSearch(): Promise<void> {
  if (usingMocks) return mockBackend.prepareSearch();
  return invoke("prepare_search");
}

/** 찾는 범위: PC 전체 / 정리 폴더와 지켜볼 폴더만 */
export function setSearchWholePc(enabled: boolean): Promise<Settings> {
  if (usingMocks) return mockBackend.setSearchWholePc(enabled);
  return invoke("set_search_whole_pc", { enabled });
}

/** 파일 열기(연결된 프로그램) */
export function openPath(path: string): Promise<void> {
  if (usingMocks) return mockBackend.openPath(path);
  return invoke("open_path", { path });
}

/** 파일 위치 열기(탐색기에서 골라 둠) */
export function revealPath(path: string): Promise<void> {
  if (usingMocks) return mockBackend.revealPath(path);
  return invoke("reveal_path", { path });
}

export function getSearchInfo(): Promise<SearchInfo> {
  if (usingMocks) return mockBackend.searchInfo();
  return invoke("search_info");
}

export function setSearchShortcut(shortcut: string): Promise<Settings> {
  if (usingMocks) return mockBackend.setSearchShortcut(shortcut);
  return invoke("set_search_shortcut", { shortcut });
}

export function listFolders(path: string): Promise<FolderEntry[]> {
  if (usingMocks) return mockBackend.listFolders(path);
  return invoke("list_folders", { path });
}

/** 폴더 바로 안의 파일(최근에 고친 것부터). 정리 폴더 안만. */
export function listFiles(path: string): Promise<FileEntry[]> {
  if (usingMocks) return mockBackend.listFiles(path);
  return invoke("list_files", { path });
}

/** 최근 정리한 파일(지금 그 자리에 있는 것만, 새것부터) */
/** 정리 기록: 넣기·옮기기·이름 바꾸기·휴지통 기록, 새것부터(최대 3,000개) */
export function listMoveLog(): Promise<LogEntry[]> {
  if (usingMocks) return mockBackend.moveLog();
  return invoke("move_log");
}

/** 정리 기록을 서식 있는 엑셀(.xlsx)로: 저장 창에서 고른 곳에 쓴다. 취소하면 false. */
export async function saveLogXlsx(defaultName: string, rows: XlsxRow[]): Promise<boolean> {
  if (usingMocks) return mockBackend.saveLogXlsx(rows);
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({ title: "정리 기록 저장", defaultPath: defaultName, filters: [{ name: "엑셀 파일", extensions: ["xlsx"] }] });
  if (!path) return false;
  await invoke("save_log_xlsx", { path, rows });
  return true;
}

export function listRecentMoves(): Promise<RecentMove[]> {
  if (usingMocks) return mockBackend.recentMoves();
  return invoke("recent_moves");
}

export function createFolder(parent: string, name: string): Promise<FolderEntry> {
  const done = usingMocks
    ? mockBackend.createFolder(parent, name)
    : invoke<FolderEntry>("create_folder", { parent, name });
  return done.then(foldersChanged);
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

/** `create`면 없는 폴더를 만든다(정리 폴더 안만: 다시 나누기의 새 폴더 제안) */
export function putFiles(paths: string[], destDir: string, create = false): Promise<PutOutcome[]> {
  if (usingMocks) return mockBackend.putFiles(paths, destDir, create);
  return invoke("put_files", { paths, destDir, create });
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
  const done = usingMocks
    ? mockBackend.renameFolder(path, newName)
    : invoke<FolderEntry>("rename_folder", { path, newName });
  return done.then(foldersChanged);
}

export function moveFolder(path: string, destParent: string): Promise<FolderEntry> {
  const done = usingMocks
    ? mockBackend.moveFolder(path, destParent)
    : invoke<FolderEntry>("move_folder", { path, destParent });
  return done.then(foldersChanged);
}

/** 폴더를 휴지통으로. 되돌리기용 번호. */
export function trashFolder(path: string): Promise<number> {
  const done = usingMocks ? mockBackend.trashFolder(path) : invoke<number>("trash_folder", { path });
  return done.then(foldersChanged);
}

/** 파일들을 휴지통으로 */
export function trashFiles(paths: string[]): Promise<TrashResult> {
  if (usingMocks) return mockBackend.trashFiles(paths);
  return invoke("trash_files", { paths });
}

/** 휴지통으로 보낸 것을 원래 자리로 */
export function restoreDeleted(deletionIds: number[]): Promise<UndoManyResult> {
  const done = usingMocks
    ? mockBackend.restoreDeleted(deletionIds)
    : invoke<UndoManyResult>("restore_deleted", { deletionIds });
  return done.then(foldersChanged);
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

/** 기존 폴더 다시 정리하기 미리보기: 고른 폴더의 파일과 넣을 곳(같은 낱말이 많으면 새 폴더 제안) */
export function folderClassifyPreview(dir: string, includeSub: boolean): Promise<FolderPreview> {
  if (usingMocks) return mockBackend.folderClassifyPreview(dir, includeSub);
  return invoke("folder_classify_preview", { dir, includeSub });
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

/** 이름 바꾸기: 폴더 안 파일(숨김·임시 파일 빼고) */
export function renameList(folder: string, includeSub: boolean): Promise<FolderFiles> {
  if (usingMocks) return mockBackend.renameList(folder, includeSub);
  return invoke("rename_list", { folder, includeSub });
}

/** 이름 바꾸기 미리보기(명렬표는 저장하지 않음) */
export function renamePreview(request: RenameRequest): Promise<RenamePreview> {
  if (usingMocks) return mockBackend.renamePreview(request);
  return invoke("rename_preview", { request });
}

/** 이름 바꾸기 실행. 되돌리기용 묶음 번호를 돌려준다. */
export function renameApply(request: RenameRequest): Promise<RenameResult> {
  if (usingMocks) return mockBackend.renameApply(request);
  return invoke("rename_apply", { request });
}

/** 이름 바꾸기 묶음 전체 되돌리기 */
export function renameUndo(batch: number): Promise<UndoManyResult> {
  if (usingMocks) return mockBackend.renameUndo(batch);
  return invoke("rename_undo", { batch });
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
