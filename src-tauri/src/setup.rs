//! 처음 실행 설정(SPEC 3-3): 정리 폴더 만들기, 예시 폴더 구조, 지켜볼 폴더, 정리 방식.
//! 폴더를 만들기만 하고 지우거나 덮어쓰지 않는다. 이미 있는 폴더·파일은 그대로 둔다.

use crate::settings::{OrganizeMode, Settings, UserKind};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// 새로 만들 때 정리 폴더 이름.
pub const ROOT_FOLDER_NAME: &str = "정리 폴더";

/// 교사용 예시 폴더(맨 위 폴더, 그 안의 폴더). 기본 목록은 SPEC 10장에서 확정 전.
pub const TEACHER_EXAMPLE: &[(&str, &[&str])] = &[
    ("업무", &["공문", "회의록", "계획서"]),
    ("수업", &[]),
    ("평가", &["수행평가", "지필고사"]),
    ("학급", &["상담", "사진", "출결"]),
    ("생활기록부", &[]),
    ("연수", &[]),
    ("개인", &[]),
];

/// 일반(회사·개인) 예시 폴더
pub const GENERAL_EXAMPLE: &[(&str, &[&str])] = &[
    ("업무", &["보고서", "회의록", "계약서"]),
    ("돈 관리", &["영수증", "세금", "보험"]),
    ("개인", &["병원", "증명서"]),
    ("공부", &[]),
    ("사진", &[]),
];

/// 쓰는 곳에 맞는 예시 폴더
pub fn example(kind: UserKind) -> &'static [(&'static str, &'static [&'static str])] {
    match kind {
        UserKind::School => TEACHER_EXAMPLE,
        UserKind::General => GENERAL_EXAMPLE,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderPreset {
    /// `기준 위치\정리 폴더`를 빈 채로 만든다.
    Empty,
    /// `기준 위치\정리 폴더`를 만들고 예시 폴더(학교·일반)를 넣는다. 이름은 예전 그대로.
    Teacher,
    /// 고른 폴더를 그대로 정리 폴더로 쓴다(아무것도 만들지 않는다).
    Existing,
}

/// 처음 설정에서 사용자가 고친 폴더 구성(폴더 이름과 그 안의 폴더들).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FolderSpec {
    pub name: String,
    #[serde(default)]
    pub children: Vec<FolderSpec>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupRequest {
    /// Empty·Teacher: 정리 폴더를 만들 위치. Existing: 정리 폴더로 쓸 폴더.
    pub base_dir: String,
    pub preset: FolderPreset,
    /// Teacher일 때 만들 폴더 구성. 없으면 교사용 예시 그대로.
    #[serde(default)]
    pub structure: Option<Vec<FolderSpec>>,
    pub watch_folders: Vec<String>,
    pub organize_mode: OrganizeMode,
    /// 쓰는 곳(없으면 학교)
    #[serde(default)]
    pub user_kind: UserKind,
}

/// 예시(학교·일반)를 폴더 구성 형태로.
pub fn example_structure(kind: UserKind) -> Vec<FolderSpec> {
    example(kind)
        .iter()
        .map(|(name, subs)| FolderSpec {
            name: name.to_string(),
            children: subs
                .iter()
                .map(|s| FolderSpec {
                    name: s.to_string(),
                    children: vec![],
                })
                .collect(),
        })
        .collect()
}

/// 폴더 이름이 올바르고, 같은 자리에 같은 이름이 없는지.
fn validate_structure(specs: &[FolderSpec]) -> Result<(), String> {
    let mut seen: Vec<String> = Vec::new();
    for spec in specs {
        crate::folders::validate_name(&spec.name)?;
        let key = spec.name.trim().to_lowercase();
        if seen.contains(&key) {
            return Err(format!(
                "같은 자리에 ‘{}’ 폴더가 두 번 있어요.",
                spec.name.trim()
            ));
        }
        seen.push(key);
        validate_structure(&spec.children)?;
    }
    Ok(())
}

fn create_specs(dir: &Path, specs: &[FolderSpec]) -> std::io::Result<()> {
    for spec in specs {
        let path = dir.join(spec.name.trim());
        fs::create_dir_all(&path)?;
        create_specs(&path, &spec.children)?;
    }
    Ok(())
}

pub fn root_path(base_dir: &Path, preset: FolderPreset) -> PathBuf {
    match preset {
        FolderPreset::Existing => base_dir.to_path_buf(),
        FolderPreset::Empty | FolderPreset::Teacher => base_dir.join(ROOT_FOLDER_NAME),
    }
}

/// 설정을 확인하고, 폴더를 만들고, 완성된 설정을 돌려준다(저장은 부르는 쪽에서).
pub fn run(req: &SetupRequest) -> Result<Settings, String> {
    let base = PathBuf::from(req.base_dir.trim());
    if req.base_dir.trim().is_empty() || !base.is_dir() {
        return Err("고른 위치를 찾을 수 없어요. 다시 골라 주세요.".into());
    }
    let root = root_path(&base, req.preset);
    let root_str = root.to_string_lossy().into_owned();
    validate_watch_folders(&req.watch_folders, &root_str)?;
    let structure = match (&req.preset, &req.structure) {
        (FolderPreset::Teacher, Some(custom)) => custom.clone(),
        (FolderPreset::Teacher, None) => example_structure(req.user_kind),
        _ => Vec::new(),
    };
    validate_structure(&structure)?;

    create_structure(&root, req.preset, &structure)
        .map_err(|e| format!("폴더를 만들지 못했어요({}): {e}", root.display()))?;

    Ok(Settings {
        setup_done: true,
        root_folder: Some(root_str),
        watch_folders: req.watch_folders.clone(),
        organize_mode: req.organize_mode,
        user_kind: req.user_kind,
        first_term_month: req.user_kind.term_months().first,
        second_term_month: req.user_kind.term_months().second,
        ..Settings::default()
    })
}

pub fn validate_watch_folders(watch: &[String], root: &str) -> Result<(), String> {
    if watch.is_empty() {
        return Err("지켜볼 폴더를 하나 이상 골라 주세요.".into());
    }
    if watch
        .iter()
        .any(|w| same_path(Path::new(w), Path::new(root)))
    {
        return Err("정리 폴더와 지켜볼 폴더는 서로 달라야 해요.".into());
    }
    Ok(())
}

pub fn create_structure(
    root: &Path,
    preset: FolderPreset,
    structure: &[FolderSpec],
) -> std::io::Result<()> {
    if preset == FolderPreset::Existing {
        return Ok(());
    }
    fs::create_dir_all(root)?;
    create_specs(root, structure)
}

/// 같은 폴더인지(Windows는 대소문자를 가리지 않으므로 실제 경로로 비교).
fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(base: &Path, preset: FolderPreset, watch: Vec<String>) -> SetupRequest {
        SetupRequest {
            base_dir: base.to_string_lossy().into_owned(),
            preset,
            structure: None,
            watch_folders: watch,
            organize_mode: OrganizeMode::Manual,
            user_kind: UserKind::School,
        }
    }

    fn downloads(dir: &Path) -> String {
        let d = dir.join("Downloads");
        fs::create_dir_all(&d).unwrap();
        d.to_string_lossy().into_owned()
    }

    #[test]
    fn teacher_preset_creates_example_tree() {
        let dir = tempfile::tempdir().unwrap();
        let watch = vec![downloads(dir.path())];
        let s = run(&request(dir.path(), FolderPreset::Teacher, watch.clone())).unwrap();

        let root = dir.path().join(ROOT_FOLDER_NAME);
        assert_eq!(s.root_folder.as_deref(), Some(&*root.to_string_lossy()));
        assert!(s.setup_done);
        assert_eq!(s.watch_folders, watch);
        assert_eq!(s.organize_mode, OrganizeMode::Manual);
        assert!(root.join("평가").join("수행평가").is_dir());
        assert!(root.join("학급").join("출결").is_dir());
        assert!(root.join("생활기록부").is_dir());
    }

    #[test]
    fn general_users_get_general_example_and_calendar_year() {
        let dir = tempfile::tempdir().unwrap();
        let watch = vec![downloads(dir.path())];
        let mut req = request(dir.path(), FolderPreset::Teacher, watch);
        req.user_kind = UserKind::General;
        let s = run(&req).unwrap();
        let root = PathBuf::from(s.root_folder.as_ref().unwrap());
        assert!(root.join("돈 관리").join("영수증").is_dir());
        assert!(!root.join("평가").exists());
        assert_eq!(s.user_kind, UserKind::General);
        // 일반: 1월부터 '올해'(학년도가 아니라 달력의 해)
        assert_eq!((s.first_term_month, s.second_term_month), (1, 7));
    }

    #[test]
    fn empty_preset_creates_only_root() {
        let dir = tempfile::tempdir().unwrap();
        run(&request(
            dir.path(),
            FolderPreset::Empty,
            vec![downloads(dir.path())],
        ))
        .unwrap();
        let root = dir.path().join(ROOT_FOLDER_NAME);
        assert!(root.is_dir());
        assert_eq!(fs::read_dir(root).unwrap().count(), 0);
    }

    #[test]
    fn existing_folder_is_used_as_is_and_files_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let mine = dir.path().join("내 자료");
        fs::create_dir_all(mine.join("수업")).unwrap();
        fs::write(mine.join("메모.txt"), b"keep").unwrap();

        let s = run(&request(
            &mine,
            FolderPreset::Existing,
            vec![downloads(dir.path())],
        ))
        .unwrap();
        assert_eq!(s.root_folder.as_deref(), Some(&*mine.to_string_lossy()));
        assert!(!mine.join(ROOT_FOLDER_NAME).exists());
        assert_eq!(fs::read(mine.join("메모.txt")).unwrap(), b"keep");
    }

    #[test]
    fn running_twice_keeps_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let watch = vec![downloads(dir.path())];
        run(&request(dir.path(), FolderPreset::Teacher, watch.clone())).unwrap();
        let file = dir
            .path()
            .join(ROOT_FOLDER_NAME)
            .join("업무")
            .join("공문")
            .join("a.hwp");
        fs::write(&file, b"data").unwrap();
        run(&request(dir.path(), FolderPreset::Teacher, watch)).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"data");
    }

    #[test]
    fn rejects_missing_base_and_empty_watch_list() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("없는 폴더");
        assert!(run(&request(
            &missing,
            FolderPreset::Empty,
            vec![downloads(dir.path())]
        ))
        .is_err());
        assert!(run(&request(dir.path(), FolderPreset::Empty, vec![])).is_err());
    }

    #[test]
    fn rejects_root_that_is_also_watched() {
        let dir = tempfile::tempdir().unwrap();
        let dl = downloads(dir.path());
        let err = run(&request(
            Path::new(&dl),
            FolderPreset::Existing,
            vec![dl.clone()],
        ));
        assert!(err.is_err());
    }

    #[test]
    fn custom_structure_is_created_and_validated() {
        let dir = tempfile::tempdir().unwrap();
        let watch = vec![downloads(dir.path())];
        let mut req = request(dir.path(), FolderPreset::Teacher, watch);
        req.structure = Some(vec![
            FolderSpec {
                name: "3학년 2반".into(),
                children: vec![FolderSpec {
                    name: "상담".into(),
                    children: vec![],
                }],
            },
            FolderSpec {
                name: " 동아리 ".into(),
                children: vec![],
            },
        ]);
        run(&req).unwrap();
        let root = dir.path().join(ROOT_FOLDER_NAME);
        assert!(root.join("3학년 2반").join("상담").is_dir());
        assert!(root.join("동아리").is_dir());
        assert!(!root.join("업무").exists());

        let dup = vec![
            FolderSpec {
                name: "a".into(),
                children: vec![],
            },
            FolderSpec {
                name: "A".into(),
                children: vec![],
            },
        ];
        req.structure = Some(dup);
        assert!(run(&req).is_err());
        req.structure = Some(vec![FolderSpec {
            name: "나쁜:이름".into(),
            children: vec![],
        }]);
        assert!(run(&req).is_err());
    }
}
