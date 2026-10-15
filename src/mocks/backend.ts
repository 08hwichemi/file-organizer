// 브라우저 미리보기용 가짜 백엔드. 실제 파일은 건드리지 않고 메모리에서만 흉내 낸다.
// 주소 뒤에 ?mock=ready 를 붙이면 처음 설정을 건너뛰고 정리함 화면부터 본다.
import type {
  AppInfo,
  FolderEntry,
  OrganizeMode,
  Settings,
  SetupInfo,
  SetupRequest,
} from "../lib/types";

const HOME = "C:\\Users\\선생님";
const SEP = "\\";

const setupInfo: SetupInfo = {
  documents: `${HOME}\\Documents`,
  desktop: `${HOME}\\Desktop`,
  downloads: `${HOME}\\Downloads`,
  rootFolderName: "정리 폴더",
  separator: SEP,
  teacherExample: [
    { name: "업무", children: ["공문", "회의록", "계획서"] },
    { name: "수업", children: [] },
    { name: "평가", children: ["수행평가", "지필고사"] },
    { name: "학급", children: ["상담", "사진", "출결"] },
    { name: "생활기록부", children: [] },
    { name: "연수", children: [] },
    { name: "개인", children: [] },
  ],
};

// 가짜 폴더: 경로 → 하위 폴더 이름과 파일 수
interface MockFolder {
  files: number;
  children: Record<string, MockFolder>;
}

const folder = (files: number, children: Record<string, MockFolder> = {}): MockFolder => ({
  files,
  children,
});

function teacherTree(): MockFolder {
  return folder(0, {
    업무: folder(4, { 공문: folder(57), 회의록: folder(18), 계획서: folder(11) }),
    수업: folder(2, { "2027 1학기": folder(46), "2027 2학기": folder(12) }),
    평가: folder(0, {
      수행평가: folder(3, { "2027": folder(0, { "1학기": folder(19), "2학기": folder(4) }) }),
      지필고사: folder(22),
    }),
    학급: folder(6, { 상담: folder(8), 사진: folder(120), 출결: folder(15) }),
    생활기록부: folder(31),
    연수: folder(9),
    개인: folder(14),
  });
}

const readyRoot = `${HOME}\\Documents\\정리 폴더`;
const startReady = new URLSearchParams(window.location.search).get("mock") === "ready";

let settings: Settings = startReady
  ? {
      setupDone: true,
      rootFolder: readyRoot,
      watchFolders: [setupInfo.downloads!, setupInfo.desktop!],
      organizeMode: "ask",
    }
  : { setupDone: false, rootFolder: null, watchFolders: [], organizeMode: "ask" };

const trees = new Map<string, MockFolder>(startReady ? [[readyRoot, teacherTree()]] : []);

function findFolder(path: string): MockFolder | null {
  for (const [rootPath, tree] of trees) {
    if (path === rootPath) return tree;
    if (!path.startsWith(rootPath + SEP)) continue;
    let node: MockFolder | undefined = tree;
    for (const part of path.slice(rootPath.length + 1).split(SEP)) {
      node = node?.children[part];
    }
    return node ?? null;
  }
  return null;
}

const delay = <T>(value: T) => new Promise<T>((resolve) => setTimeout(() => resolve(value), 120));
const fail = (message: string) => new Promise<never>((_, reject) => setTimeout(() => reject(message), 120));

let pickCount = 0;

export const mockBackend = {
  appInfo: () => delay<AppInfo>({ name: "파일 정리 도우미", version: "0.1.0", os: "browser" }),
  setupInfo: () => delay(setupInfo),
  getSettings: () => delay({ ...settings }),

  completeSetup(req: SetupRequest) {
    if (req.watchFolders.length === 0) return fail("지켜볼 폴더를 하나 이상 골라 주세요.");
    const root =
      req.preset === "existing" ? req.baseDir : `${req.baseDir}${SEP}${setupInfo.rootFolderName}`;
    if (req.watchFolders.includes(root)) return fail("정리 폴더와 지켜볼 폴더는 서로 달라야 해요.");
    if (!trees.has(root)) trees.set(root, req.preset === "empty" ? folder(0) : teacherTree());
    settings = {
      setupDone: true,
      rootFolder: root,
      watchFolders: req.watchFolders,
      organizeMode: req.organizeMode,
    };
    return delay({ ...settings });
  },

  setOrganizeMode(mode: OrganizeMode) {
    settings = { ...settings, organizeMode: mode };
    return delay({ ...settings });
  },

  setWatchFolders(folders: string[]) {
    if (folders.length === 0) return fail("지켜볼 폴더를 하나 이상 골라 주세요.");
    settings = { ...settings, watchFolders: folders };
    return delay({ ...settings });
  },

  listFolders(path: string) {
    const node = findFolder(path);
    if (!node) return fail("폴더를 읽지 못했어요.");
    const entries: FolderEntry[] = Object.entries(node.children)
      .map(([name, child]) => ({
        name,
        path: path + SEP + name,
        fileCount: child.files,
        folderCount: Object.keys(child.children).length,
      }))
      .sort((a, b) => a.name.localeCompare(b.name, "ko"));
    return delay(entries);
  },

  pickFolder() {
    pickCount += 1;
    return delay(`${HOME}\\Documents\\고른 폴더 ${pickCount}`);
  },
};
