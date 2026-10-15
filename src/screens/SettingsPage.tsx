import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check } from "lucide-react";
import ModeChoice from "../components/ModeChoice";
import WatchFolderEditor from "../components/WatchFolderEditor";
import { errorMessage, getAppInfo } from "../lib/backend";
import type { OrganizeMode } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import { useUpdateStore } from "../store/useUpdateStore";
import "./SettingsPage.css";

// 설정(SPEC 3-3). 바꾸면 바로 저장한다. 나머지 항목은 6단계에서 늘어난다.
export default function SettingsPage() {
  const settings = useAppStore((s) => s.settings);
  const saveMode = useAppStore((s) => s.setOrganizeMode);
  const saveWatch = useAppStore((s) => s.setWatchFolders);
  const known = useAppStore((s) => s.setupInfo);
  const [watchDraft, setWatchDraft] = useState<string[]>(settings?.watchFolders ?? []);
  const [saved, setSaved] = useState<"mode" | "watch" | null>(null);
  const [errors, setErrors] = useState<{ mode?: string; watch?: string }>({});
  const [version, setVersion] = useState<string | null>(null);
  const savedTimer = useRef<number>();

  useEffect(() => {
    getAppInfo()
      .then((info) => setVersion(info.version))
      .catch(() => setVersion(null));
    return () => window.clearTimeout(savedTimer.current);
  }, []);

  if (!settings) return null;

  const flashSaved = (which: "mode" | "watch") => {
    setSaved(which);
    window.clearTimeout(savedTimer.current);
    savedTimer.current = window.setTimeout(() => setSaved(null), 2000);
  };

  const changeMode = async (mode: OrganizeMode) => {
    if (mode === settings.organizeMode) return;
    try {
      await saveMode(mode);
      setErrors((e) => ({ ...e, mode: undefined }));
      flashSaved("mode");
    } catch (e) {
      setErrors((prev) => ({ ...prev, mode: errorMessage(e) }));
    }
  };

  const changeWatch = async (folders: string[]) => {
    setWatchDraft(folders);
    if (folders.length === 0) {
      setErrors((e) => ({ ...e, watch: "지켜볼 폴더를 하나 이상 골라 주세요." }));
      return;
    }
    try {
      await saveWatch(folders);
      setErrors((e) => ({ ...e, watch: undefined }));
      flashSaved("watch");
    } catch (e) {
      setErrors((prev) => ({ ...prev, watch: errorMessage(e) }));
    }
  };

  return (
    <div className="page settings">
      <h1 className="page__title">설정</h1>

      <Section title="새 파일이 생기면" saved={saved === "mode"} error={errors.mode}>
        <ModeChoice value={settings.organizeMode} onChange={changeMode} />
      </Section>

      <Section
        title="지켜볼 폴더"
        description="골라 둔 폴더에 바로 저장되는 파일만 봐요."
        saved={saved === "watch"}
        error={errors.watch}
      >
        <WatchFolderEditor known={known} value={watchDraft} onChange={changeWatch} />
      </Section>

      <Section title="정리 폴더" description="파일을 넣어 두는 폴더예요. 처음 설정에서 정했어요.">
        <div className="settings__path">{settings.rootFolder}</div>
      </Section>

      <Section title="업데이트" description="새 버전이 나오면 프로그램이 알아서 알려 드려요(켤 때와 하루에 한 번 확인).">
        <UpdateRow version={version} />
      </Section>
    </div>
  );
}

function Section({
  title,
  description,
  saved,
  error,
  children,
}: {
  title: string;
  description?: string;
  saved?: boolean;
  error?: string;
  children: ReactNode;
}) {
  return (
    <section className="settings__section">
      <div className="settings__head">
        <h2 className="settings__title">{title}</h2>
        <span className="settings__saved" aria-live="polite">
          {saved && (
            <>
              <Check size={14} aria-hidden="true" />
              저장했어요
            </>
          )}
        </span>
      </div>
      {description && <p className="settings__desc">{description}</p>}
      {children}
      {error && (
        <p className="error-text" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}

function UpdateRow({ version }: { version: string | null }) {
  const { status, version: newVersion, error, check } = useUpdateStore();
  const message =
    status === "checking"
      ? "확인하는 중…"
      : status === "latest"
        ? "지금 최신 버전이에요."
        : status === "available" || status === "downloading"
          ? `새 버전 ${newVersion}이 있어요. 위의 ‘지금 업데이트’를 눌러 주세요.`
          : status === "error"
            ? error
            : null;
  return (
    <div className="settings__update button-row">
      <div className="settings__update-text">
        <div>지금 버전 {version ?? "…"}</div>
        {message && <div className="settings__update-msg">{message}</div>}
      </div>
      <button type="button" className="btn" onClick={() => void check()} disabled={status === "checking" || status === "downloading"}>
        업데이트 확인
      </button>
    </div>
  );
}
