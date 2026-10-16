import { useMemo, useState, type ReactNode } from "react";
import { FileText, FolderInput, FolderPlus, FolderTree, Monitor } from "lucide-react";
import ChoiceCard from "../components/ChoiceCard";
import FolderIcon from "../components/FolderIcon";
import ModeChoice from "../components/ModeChoice";
import StructureEditor, { structureProblem } from "../components/StructureEditor";
import WatchFolderEditor from "../components/WatchFolderEditor";
import { errorMessage, pickFolder } from "../lib/backend";
import { friendlyName, joinPath, samePath } from "../lib/paths";
import type { FolderSpec, OrganizeMode, SetupInfo } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import "./FirstRun.css";

type Location = "documents" | "desktop" | "custom" | "existing";
type StepKey = "location" | "structure" | "watch" | "mode" | "confirm";

// 처음 실행(SPEC 3-3): 정리 폴더 → 폴더 구성 → 지켜볼 폴더 → 새 파일 알림 방식 → 시작.
export default function FirstRun() {
  const finishSetup = useAppStore((s) => s.finishSetup);
  const info = useAppStore((s) => s.setupInfo);

  const [stepIndex, setStepIndex] = useState(0);
  const [location, setLocation] = useState<Location>(
    info?.documents ? "documents" : info?.desktop ? "desktop" : "custom",
  );
  const [customDir, setCustomDir] = useState<string | null>(null);
  const [existingDir, setExistingDir] = useState<string | null>(null);
  const [preset, setPreset] = useState<"teacher" | "empty">("teacher");
  const [structure, setStructure] = useState<FolderSpec[]>(() => exampleStructure(info));
  const [watch, setWatch] = useState<string[]>(() =>
    [info?.downloads, info?.desktop].filter((p): p is string => !!p),
  );
  const [mode, setMode] = useState<OrganizeMode>("ask");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const steps: StepKey[] =
    location === "existing"
      ? ["location", "watch", "mode", "confirm"]
      : ["location", "structure", "watch", "mode", "confirm"];
  const step = steps[Math.min(stepIndex, steps.length - 1)];

  const baseDir =
    location === "documents"
      ? info?.documents
      : location === "desktop"
        ? info?.desktop
        : location === "custom"
          ? customDir
          : existingDir;

  const rootPreview = useMemo(() => {
    if (!info || !baseDir) return null;
    return location === "existing" ? baseDir : joinPath(baseDir, info.rootFolderName, info.separator);
  }, [info, baseDir, location]);

  const watchProblem =
    watch.length === 0
      ? "지켜볼 폴더를 하나 이상 골라 주세요."
      : rootPreview && watch.some((w) => samePath(w, rootPreview))
        ? "정리 폴더와 지켜볼 폴더는 서로 달라야 해요."
        : null;

  const structureError = preset === "teacher" ? structureProblem(structure) : null;

  const canGoNext =
    step === "location"
      ? !!baseDir
      : step === "watch"
        ? !watchProblem
        : step === "structure"
          ? !structureError
          : true;

  const choose = async (which: "custom" | "existing") => {
    setError(null);
    try {
      const title = which === "custom" ? "정리 폴더를 만들 위치 고르기" : "정리 폴더로 쓸 폴더 고르기";
      const picked = await pickFolder(title);
      if (!picked) return;
      if (which === "custom") setCustomDir(picked);
      else setExistingDir(picked);
      setLocation(which);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const start = async () => {
    if (!baseDir) return;
    setBusy(true);
    setError(null);
    try {
      await finishSetup({
        baseDir,
        preset: location === "existing" ? "existing" : preset,
        structure: location !== "existing" && preset === "teacher" ? structure : undefined,
        watchFolders: watch,
        organizeMode: mode,
      });
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  };

  const goTo = (key: StepKey) => setStepIndex(steps.indexOf(key));

  if (!info) return null;

  return (
    <main className="setup">
      <div className="setup__card">
        <header className="setup__head">
          <div className="setup__brand">
            <span className="setup__logo" aria-hidden="true">
              <FolderIcon size={18} />
            </span>
            <span>파일 정리 도우미 · 처음 설정</span>
          </div>
          <span className="setup__count">
            {stepIndex + 1} / {steps.length}
          </span>
        </header>
        <div className="setup__progress" aria-hidden="true">
          <span style={{ width: `${((stepIndex + 1) / steps.length) * 100}%` }} />
        </div>

        <div className="setup__content">
          {step === "location" && (
            <>
              <h1 className="setup__title">파일을 정리해 둘 폴더를 정해 주세요</h1>
              <p className="setup__lead">
                이 폴더 안에 하위 폴더를 만들어 파일을 넣어요. 앞으로 ‘정리 폴더’라고 부를게요.
              </p>
              <div className="stack-12" role="radiogroup" aria-label="정리 폴더 위치">
                {info.documents && (
                  <ChoiceCard
                    name="location"
                    checked={location === "documents"}
                    onSelect={() => setLocation("documents")}
                    icon={<FileText size={20} />}
                    title="문서 폴더에 새로 만들기"
                    description={joinPath(info.documents, info.rootFolderName, info.separator)}
                  />
                )}
                {info.desktop && (
                  <ChoiceCard
                    name="location"
                    checked={location === "desktop"}
                    onSelect={() => setLocation("desktop")}
                    icon={<Monitor size={20} />}
                    title="바탕화면에 새로 만들기"
                    description={joinPath(info.desktop, info.rootFolderName, info.separator)}
                  />
                )}
                <ChoiceCard
                  name="location"
                  checked={location === "custom"}
                  onSelect={() => void choose("custom")}
                  icon={<FolderPlus size={20} />}
                  title="다른 곳에 새로 만들기"
                  description={
                    customDir
                      ? `${joinPath(customDir, info.rootFolderName, info.separator)} · 누르면 다시 골라요`
                      : "누르면 위치를 고르는 창이 열려요"
                  }
                />
                <ChoiceCard
                  name="location"
                  checked={location === "existing"}
                  onSelect={() => void choose("existing")}
                  icon={<FolderInput size={20} />}
                  title="쓰던 폴더 그대로 쓰기"
                  description={
                    existingDir
                      ? `${existingDir} · 누르면 다시 골라요`
                      : "이미 폴더를 나눠 쓰고 있다면 그 폴더를 골라 주세요. 안의 파일은 그대로 둬요."
                  }
                />
              </div>
            </>
          )}

          {step === "structure" && (
            <>
              <h1 className="setup__title">정리 폴더 안을 어떻게 시작할까요?</h1>
              <p className="setup__lead">폴더는 나중에 정리함이나 설정에서도 언제든 만들고, 이름을 바꾸고, 옮길 수 있어요.</p>
              <div className="stack-12" role="radiogroup" aria-label="폴더 구성">
                <ChoiceCard
                  name="preset"
                  checked={preset === "teacher"}
                  onSelect={() => setPreset("teacher")}
                  icon={<FolderTree size={20} />}
                  title="폴더를 정해서 시작"
                  description="교사용 예시를 고쳐서 써요. 이름을 바꾸고, 더하고, 뺄 수 있어요."
                />
                {preset === "teacher" && (
                  <div className="setup__structure">
                    <div className="setup__structure-head">
                      <span>만들 폴더</span>
                      <button type="button" className="btn btn--link" onClick={() => setStructure(exampleStructure(info))}>
                        예시로 되돌리기
                      </button>
                    </div>
                    <StructureEditor value={structure} onChange={setStructure} />
                    {structureError && <p className="error-text">{structureError}</p>}
                  </div>
                )}
                <ChoiceCard
                  name="preset"
                  checked={preset === "empty"}
                  onSelect={() => setPreset("empty")}
                  icon={<FolderPlus size={20} />}
                  title="빈 폴더로 시작"
                  description="정리 폴더만 만들고, 안의 폴더는 쓰면서 직접 만들어요."
                />
              </div>
            </>
          )}

          {step === "watch" && (
            <>
              <h1 className="setup__title">어느 폴더에 들어오는 새 파일을 정리할까요?</h1>
              <p className="setup__lead">
                골라 둔 폴더에 바로 저장되는 파일만 봐요. 그 안의 하위 폴더는 보지 않아요.
              </p>
              <WatchFolderEditor known={info} value={watch} onChange={setWatch} />
              {watchProblem && <p className="error-text">{watchProblem}</p>}
            </>
          )}

          {step === "mode" && (
            <>
              <h1 className="setup__title">새 파일이 생기면 어떻게 할까요?</h1>
              <p className="setup__lead">설정에서 언제든 바꿀 수 있어요.</p>
              <ModeChoice value={mode} onChange={setMode} />
            </>
          )}

          {step === "confirm" && (
            <>
              <h1 className="setup__title">이렇게 시작할게요</h1>
              <p className="setup__lead">‘시작하기’를 누르면 정리 폴더를 만들어요. 이미 있는 폴더와 파일은 그대로 둬요.</p>
              <dl className="setup__summary">
                <SummaryRow label="정리 폴더" onEdit={() => goTo("location")}>
                  <span className="setup__path">{rootPreview}</span>
                  <span className="setup__sub">
                    {location === "existing" ? "쓰던 폴더를 그대로 써요" : "새로 만들어요"}
                  </span>
                </SummaryRow>
                {location !== "existing" && (
                  <SummaryRow label="폴더 구성" onEdit={() => goTo("structure")}>
                    {preset === "teacher"
                      ? structure.length > 0
                        ? structure.map((f) => f.name.trim()).join(" · ")
                        : "빈 폴더"
                      : "빈 폴더"}
                  </SummaryRow>
                )}
                <SummaryRow label="지켜볼 폴더" onEdit={() => goTo("watch")}>
                  {watch.map((w) => friendlyName(w, info)).join(" · ")}
                </SummaryRow>
                <SummaryRow label="새 파일이 생기면" onEdit={() => goTo("mode")}>
                  {mode === "ask" ? "바로 물어보기" : "모아 두고 나중에 정리"}
                </SummaryRow>
              </dl>
            </>
          )}

          {error && (
            <p className="error-text" role="alert">
              {error}
            </p>
          )}
        </div>

        <footer className="setup__foot button-row">
          <button
            type="button"
            className="btn btn--ghost"
            onClick={() => setStepIndex((i) => i - 1)}
            disabled={stepIndex === 0 || busy}
            style={{ visibility: stepIndex === 0 ? "hidden" : "visible" }}
          >
            이전
          </button>
          {step === "confirm" ? (
            <button type="button" className="btn btn--primary" onClick={start} disabled={busy}>
              {busy ? "만드는 중…" : "시작하기"}
            </button>
          ) : (
            <button
              type="button"
              className="btn btn--primary"
              onClick={() => setStepIndex((i) => i + 1)}
              disabled={!canGoNext}
            >
              다음
            </button>
          )}
        </footer>
      </div>
    </main>
  );
}

function SummaryRow({
  label,
  onEdit,
  children,
}: {
  label: string;
  onEdit: () => void;
  children: ReactNode;
}) {
  return (
    <div className="setup__summary-row">
      <dt>{label}</dt>
      <dd>{children}</dd>
      <button type="button" className="btn btn--link" onClick={onEdit} aria-label={`${label} 바꾸기`}>
        바꾸기
      </button>
    </div>
  );
}

function exampleStructure(info: SetupInfo | null): FolderSpec[] {
  return (info?.teacherExample ?? []).map((f) => ({
    name: f.name,
    children: f.children.map((c) => ({ name: c, children: [] })),
  }));
}
