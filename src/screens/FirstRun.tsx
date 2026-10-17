import { useMemo, useState, type ReactNode } from "react";
import { FileText, FolderCheck, FolderInput, FolderPlus, FolderTree, House, Monitor, School, X } from "lucide-react";
import ChoiceCard from "../components/ChoiceCard";
import FolderIcon from "../components/FolderIcon";
import ModeChoice from "../components/ModeChoice";
import StructureEditor, { structureProblem } from "../components/StructureEditor";
import WatchFolderEditor from "../components/WatchFolderEditor";
import { errorMessage, pickFolder } from "../lib/backend";
import { friendlyName, joinPath, samePath } from "../lib/paths";
import type { FolderSpec, OrganizeMode, SetupInfo, UserKind } from "../lib/types";
import { useAppStore } from "../store/useAppStore";
import "./FirstRun.css";

/** current = 다시 설정할 때 지금 정리 폴더 그대로 */
type Location = "current" | "documents" | "desktop" | "desktopAsIs" | "custom" | "existing";
type StepKey = "kind" | "location" | "structure" | "watch" | "mode" | "confirm";

// 처음 실행(SPEC 3-3): 쓰는 곳 → 정리 폴더 → 폴더 구성 → 지켜볼 폴더 → 새 파일 알림 방식 → 시작.
// 설정의 "처음 설정 다시 하기"로 열면(redo) 지금 설정으로 채워 두고, 바꾼 것만 적용한다(규칙·단어·기록·파일은 그대로).
export default function FirstRun() {
  const finishSetup = useAppStore((s) => s.finishSetup);
  const redoSetup = useAppStore((s) => s.redoSetup);
  const info = useAppStore((s) => s.setupInfo);
  const redo = useAppStore((s) => s.status === "redo");
  const current = useAppStore((s) => (s.status === "redo" ? s.settings : null));
  const currentRoot = current?.rootFolder ?? null;

  const [stepIndex, setStepIndex] = useState(0);
  const [location, setLocation] = useState<Location>(
    currentRoot ? "current" : info?.documents ? "documents" : info?.desktop ? "desktop" : "custom",
  );
  const [customDir, setCustomDir] = useState<string | null>(null);
  const [existingDir, setExistingDir] = useState<string | null>(null);
  const [kind, setKind] = useState<UserKind>(current?.userKind ?? "school");
  // 쓰던 폴더(지금 정리 폴더 포함)는 기본으로 폴더를 더 만들지 않는다
  const [preset, setPreset] = useState<"teacher" | "empty">(currentRoot ? "empty" : "teacher");
  const [structure, setStructure] = useState<FolderSpec[]>(() => exampleStructure(info, current?.userKind ?? "school"));
  const [watch, setWatch] = useState<string[]>(() =>
    current?.watchFolders.length
      ? current.watchFolders
      : [info?.downloads, info?.desktop].filter((p): p is string => !!p),
  );
  const [mode, setMode] = useState<OrganizeMode>(current?.organizeMode ?? "ask");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /** 고른 폴더를 그대로 정리 폴더로(새로 만들지 않음) */
  const asIs = location === "existing" || location === "current" || location === "desktopAsIs";
  const steps: StepKey[] = ["kind", "location", "structure", "watch", "mode", "confirm"];
  const step = steps[Math.min(stepIndex, steps.length - 1)];

  const baseDir =
    location === "current"
      ? currentRoot
      : location === "documents"
        ? info?.documents
        : location === "desktop" || location === "desktopAsIs"
          ? info?.desktop
          : location === "custom"
            ? customDir
            : existingDir;

  /** 그 위치에 새로 만들 정리 폴더가 지금 정리 폴더와 같으면 그 칸은 숨긴다(‘지금 정리 폴더 그대로’와 겹침) */
  const isCurrent = (dir: string) =>
    !!info && !!currentRoot && samePath(joinPath(dir, info.rootFolderName, info.separator), currentRoot);

  const rootPreview = useMemo(() => {
    if (!info || !baseDir) return null;
    return asIs ? baseDir : joinPath(baseDir, info.rootFolderName, info.separator);
  }, [info, baseDir, asIs]);

  const watchProblem = watch.length === 0 ? "지켜볼 폴더를 하나 이상 골라 주세요." : null;

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
      setPreset(which === "custom" ? "teacher" : "empty");
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
        preset: asIs ? "existing" : preset,
        // 쓰던 폴더에 "예시 폴더 더하기"를 고르면 없는 폴더만 만든다
        structure: preset === "teacher" ? structure : undefined,
        watchFolders: watch,
        organizeMode: mode,
        userKind: kind,
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
            <span>AutoFolder · {redo ? "처음 설정 다시 하기" : "처음 설정"}</span>
          </div>
          <span className="setup__count">
            {stepIndex + 1} / {steps.length}
          </span>
          {redo && (
            <button
              type="button"
              className="btn btn--ghost setup__cancel"
              onClick={() => redoSetup(false)}
              disabled={busy}
            >
              <X size={16} aria-hidden="true" />
              취소
            </button>
          )}
        </header>
        <div className="setup__progress" aria-hidden="true">
          <span style={{ width: `${((stepIndex + 1) / steps.length) * 100}%` }} />
        </div>

        <div className="setup__content">
          {step === "kind" && (
            <>
              <h1 className="setup__title">어디에서 쓰시나요?</h1>
              <p className="setup__lead">
                {redo
                  ? "지금 설정이 골라져 있어요. 바꿀 것만 바꾸고 ‘다음’을 누르세요. 정리 규칙·자동 분류 단어·기록과 폴더·파일은 그대로 둬요."
                  : "고른 쪽에 맞춰 예시 폴더와 파일을 알아서 나눌 때 쓰는 낱말을 준비해 드려요. 설정에서 언제든 바꿀 수 있어요."}
              </p>
              <div className="stack-12" role="radiogroup" aria-label="쓰는 곳">
                <ChoiceCard
                  name="kind"
                  checked={kind === "school"}
                  onSelect={() => {
                    setKind("school");
                    setStructure(exampleStructure(info, "school"));
                  }}
                  icon={<School size={20} />}
                  title="학교"
                  description="선생님용: 업무·수업·평가·학급·생활기록부 폴더, 공문·수행평가 같은 낱말, 학년도·학기(3월·9월)"
                />
                <ChoiceCard
                  name="kind"
                  checked={kind === "general"}
                  onSelect={() => {
                    setKind("general");
                    setStructure(exampleStructure(info, "general"));
                  }}
                  icon={<House size={20} />}
                  title="집·개인"
                  description="문서·사진·동영상·음악·압축·설치처럼 파일 종류로 나눠요. 연도는 달력의 해"
                />
              </div>
            </>
          )}

          {step === "location" && (
            <>
              <h1 className="setup__title">파일을 정리해 둘 폴더를 정해 주세요</h1>
              <p className="setup__lead">
                이 폴더 안에 하위 폴더를 만들어 파일을 넣어요. 앞으로 ‘정리 폴더’라고 부를게요.
              </p>
              <div className="stack-12" role="radiogroup" aria-label="정리 폴더 위치">
                {currentRoot && (
                  <ChoiceCard
                    name="location"
                    checked={location === "current"}
                    onSelect={() => {
                      setLocation("current");
                      setPreset("empty");
                    }}
                    icon={<FolderCheck size={20} />}
                    title="지금 정리 폴더 그대로"
                    description={currentRoot}
                  />
                )}
                {info.documents && !isCurrent(info.documents) && (
                  <ChoiceCard
                    name="location"
                    checked={location === "documents"}
                    onSelect={() => {
                      setLocation("documents");
                      setPreset("teacher");
                    }}
                    icon={<FileText size={20} />}
                    title="문서 폴더에 새로 만들기"
                    description={joinPath(info.documents, info.rootFolderName, info.separator)}
                  />
                )}
                {info.desktop && !isCurrent(info.desktop) && (
                  <ChoiceCard
                    name="location"
                    checked={location === "desktop"}
                    onSelect={() => {
                      setLocation("desktop");
                      setPreset("teacher");
                    }}
                    icon={<Monitor size={20} />}
                    title="바탕화면에 새로 만들기"
                    description={joinPath(info.desktop, info.rootFolderName, info.separator)}
                  />
                )}
                {/* 바탕화면을 그대로 정리 폴더로: 바탕화면에 바로 저장된 파일을 바탕화면 안의 폴더로 넣는다 */}
                {info.desktop && !(currentRoot && samePath(currentRoot, info.desktop)) && (
                  <ChoiceCard
                    name="location"
                    checked={location === "desktopAsIs"}
                    onSelect={() => {
                      setLocation("desktopAsIs");
                      setPreset("empty");
                    }}
                    icon={<Monitor size={20} />}
                    title="바탕화면 그대로 쓰기"
                    description="바탕화면에 있는 폴더들로 정리해요. 정리하다 새로 만드는 폴더도 바탕화면에 생겨요."
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
              <h1 className="setup__title">
                {asIs ? "정리 폴더 안에 폴더를 더 만들까요?" : "정리 폴더 안을 어떻게 시작할까요?"}
              </h1>
              <p className="setup__lead">
                {asIs
                  ? "있던 폴더와 파일은 그대로 두고, 없는 폴더만 만들어요. 폴더는 나중에 정리함이나 설정에서도 만들 수 있어요."
                  : "폴더는 나중에 정리함이나 설정에서도 언제든 만들고, 이름을 바꾸고, 옮길 수 있어요."}
              </p>
              <div className="stack-12" role="radiogroup" aria-label="폴더 구성">
                <ChoiceCard
                  name="preset"
                  checked={preset === "teacher"}
                  onSelect={() => setPreset("teacher")}
                  icon={<FolderTree size={20} />}
                  title={asIs ? "예시 폴더 더하기" : "폴더를 정해서 시작"}
                  description={`${kind === "general" ? "집·개인" : "교사용"} 예시를 고쳐서 써요. 이름을 바꾸고, 더하고, 뺄 수 있어요.${asIs ? " 이미 있는 폴더는 건너뛰어요." : ""}`}
                />
                {preset === "teacher" && (
                  <div className="setup__structure">
                    <div className="setup__structure-head">
                      <span>만들 폴더</span>
                      <button type="button" className="btn btn--link" onClick={() => setStructure(exampleStructure(info, kind))}>
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
                  title={asIs ? "폴더는 지금 그대로" : "빈 폴더로 시작"}
                  description={
                    asIs
                      ? "아무 폴더도 만들지 않아요."
                      : "정리 폴더만 만들고, 안의 폴더는 쓰면서 직접 만들어요."
                  }
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
              <h1 className="setup__title">{redo ? "이렇게 바꿀게요" : "이렇게 시작할게요"}</h1>
              <p className="setup__lead">
                {redo
                  ? "‘적용하기’를 누르면 바꾼 것만 적용해요. 정리 규칙·자동 분류 단어·기록과 이미 있는 폴더·파일은 그대로 둬요."
                  : "‘시작하기’를 누르면 정리 폴더를 만들어요. 이미 있는 폴더와 파일은 그대로 둬요."}
              </p>
              <dl className="setup__summary">
                <SummaryRow label="쓰는 곳" onEdit={() => goTo("kind")}>
                  {kind === "general" ? "집·개인" : "학교"}
                </SummaryRow>
                <SummaryRow label="정리 폴더" onEdit={() => goTo("location")}>
                  <span className="setup__path">{location === "desktopAsIs" ? "바탕화면" : rootPreview}</span>
                  <span className="setup__sub">
                    {location === "current"
                      ? "지금 정리 폴더 그대로"
                      : location === "existing"
                        ? "쓰던 폴더를 그대로 써요"
                        : location === "desktopAsIs"
                          ? "바탕화면을 그대로 써요(바탕화면의 폴더로 정리)"
                          : "새로 만들어요"}
                  </span>
                </SummaryRow>
                <SummaryRow label="폴더 구성" onEdit={() => goTo("structure")}>
                  {preset === "teacher" && structure.length > 0
                    ? `${structure.map((f) => f.name.trim()).join(" · ")}${asIs ? " (없는 것만 만들어요)" : ""}`
                    : asIs
                      ? "지금 그대로"
                      : "빈 폴더"}
                </SummaryRow>
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
              {busy ? (redo ? "적용하는 중…" : "만드는 중…") : redo ? "적용하기" : "시작하기"}
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

function exampleStructure(info: SetupInfo | null, kind: UserKind): FolderSpec[] {
  const example = kind === "general" ? info?.generalExample : info?.teacherExample;
  return (example ?? []).map((f) => ({
    name: f.name,
    children: f.children.map((c) => ({ name: c, children: [] })),
  }));
}
