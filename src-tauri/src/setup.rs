//! 처음 실행 설정(SPEC 3-3): 정리 폴더 만들기, 예시 폴더 구조, 지켜볼 폴더, 정리 방식.
//! 폴더를 만들기만 하고 지우거나 덮어쓰지 않는다. 이미 있는 폴더·파일은 그대로 둔다.

use crate::settings::{OrganizeMode, Settings, UserKind};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// 새로 만들 때 정리 폴더 이름.
pub const ROOT_FOLDER_NAME: &str = "정리 폴더";

/// 교사용 예시 폴더(맨 위 폴더, 그 안의 폴더). 큰 분야만 만들고, 안쪽 폴더(업무 › 공문 등)는
/// 그런 파일이 처음 들어올 때 자동 분류가 만든다(사람마다 안 쓰는 빈 폴더가 생기지 않게, 사용자 요청).
pub const TEACHER_EXAMPLE: &[(&str, &[&str])] = &[
    ("업무", &[]),
    ("수업", &[]),
    ("평가", &[]),
    ("학급", &[]),
    ("생활기록부", &[]),
    ("연수", &[]),
    ("개인", &[]),
];

/// 집·개인 예시 폴더: 파일 종류(사용자 결정 — 쓰임새로 나누면 사람마다 맞지 않음).
/// 이름은 classify::kind_folder와 같다.
pub const GENERAL_EXAMPLE: &[(&str, &[&str])] = &[
    ("문서", &[]),
    ("사진", &[]),
    ("동영상", &[]),
    ("음악", &[]),
    ("압축·설치", &[]),
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
    /// Teacher·Existing일 때 만들 폴더 구성(없는 폴더만 만든다). Teacher인데 없으면 예시 그대로.
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

/// 처음 설정: 기본 설정 위에 고른 것을 얹는다.
#[cfg(test)]
pub fn run(req: &SetupRequest) -> Result<Settings, String> {
    run_over(req, Settings::default())
}

/// 설정을 확인하고, 폴더를 만들고, 지금 설정(`current`) 위에 고른 것만 바꾼 설정을 돌려준다(저장은 부르는 쪽에서).
/// "처음 설정 다시 하기"에서도 쓴다: 정리 규칙·자동 분류 단어·기록·자주 쓰는 폴더·검색 등은 그대로 두고,
/// 학기 시작 달은 쓰는 곳이 바뀔 때만, 직접 고른 자주 쓰는 폴더는 정리 폴더가 바뀔 때만 비운다.
pub fn run_over(req: &SetupRequest, current: Settings) -> Result<Settings, String> {
    let base = PathBuf::from(req.base_dir.trim());
    if req.base_dir.trim().is_empty() || !base.is_dir() {
        return Err("고른 위치를 찾을 수 없어요. 다시 골라 주세요.".into());
    }
    let root = root_path(&base, req.preset);
    let root_str = root.to_string_lossy().into_owned();
    validate_watch_folders(&req.watch_folders, &root_str)?;
    let structure = match (&req.preset, &req.structure) {
        (FolderPreset::Empty, _) => Vec::new(),
        (_, Some(custom)) => custom.clone(),
        (FolderPreset::Teacher, None) => example_structure(req.user_kind),
        (FolderPreset::Existing, None) => Vec::new(),
    };
    validate_structure(&structure)?;

    create_structure(&root, req.preset, &structure)
        .map_err(|e| format!("폴더를 만들지 못했어요({}): {e}", root.display()))?;

    let kind_changed = !current.setup_done || current.user_kind != req.user_kind;
    let root_changed = current
        .root_folder
        .as_deref()
        .is_none_or(|old| !same_path(Path::new(old), &root));
    let mut s = current;
    s.setup_done = true;
    s.root_folder = Some(root_str);
    s.watch_folders = req.watch_folders.clone();
    s.organize_mode = req.organize_mode;
    s.user_kind = req.user_kind;
    if kind_changed {
        let m = req.user_kind.term_months();
        s.first_term_month = m.first;
        s.second_term_month = m.second;
    }
    if root_changed {
        s.pinned_favorites.clear();
    }
    Ok(s)
}

/// 지켜볼 폴더가 하나 이상인지. 정리 폴더와 같아도 된다(바탕화면 그대로 쓰기: 바탕화면에 바로 저장된 파일만
/// 새 파일로 보고, 바탕화면 안의 폴더로 넣는다 — 감시는 그 폴더 바로 안의 파일만 본다).
pub fn validate_watch_folders(watch: &[String], _root: &str) -> Result<(), String> {
    if watch.is_empty() {
        return Err("지켜볼 폴더를 하나 이상 골라 주세요.".into());
    }
    Ok(())
}

pub fn create_structure(
    root: &Path,
    preset: FolderPreset,
    structure: &[FolderSpec],
) -> std::io::Result<()> {
    // 쓰던 폴더: 그 안에 없는 폴더만 더한다(고른 구성이 없으면 아무것도 만들지 않음)
    if preset != FolderPreset::Existing {
        fs::create_dir_all(root)?;
    }
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
        assert!(root.join("평가").is_dir());
        assert!(root.join("생활기록부").is_dir());
        // 안쪽 폴더는 미리 만들지 않는다(파일이 들어올 때 자동 분류가 만듦)
        assert!(!root.join("평가").join("수행평가").exists());
    }

    #[test]
    fn general_users_get_general_example_and_calendar_year() {
        let dir = tempfile::tempdir().unwrap();
        let watch = vec![downloads(dir.path())];
        let mut req = request(dir.path(), FolderPreset::Teacher, watch);
        req.user_kind = UserKind::General;
        let s = run(&req).unwrap();
        let root = PathBuf::from(s.root_folder.as_ref().unwrap());
        assert!(root.join("문서").is_dir());
        assert!(root.join("압축·설치").is_dir());
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
        let file = dir.path().join(ROOT_FOLDER_NAME).join("업무").join("a.hwp");
        fs::write(&file, b"data").unwrap();
        run(&request(dir.path(), FolderPreset::Teacher, watch)).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"data");
    }

    #[test]
    fn redo_keeps_other_settings_and_adds_only_missing_folders() {
        let dir = tempfile::tempdir().unwrap();
        let watch = vec![downloads(dir.path())];
        let first = run(&request(dir.path(), FolderPreset::Teacher, watch.clone())).unwrap();
        let root = PathBuf::from(first.root_folder.clone().unwrap());
        fs::write(root.join("업무").join("a.hwp"), b"data").unwrap();
        let current = Settings {
            pinned_favorites: vec![Some(root.join("업무").to_string_lossy().into_owned())],
            search_whole_pc: false,
            first_term_month: 2,
            ..first
        };

        // 같은 정리 폴더 그대로 + 일반 예시 더하기
        let mut req = request(&root, FolderPreset::Existing, watch.clone());
        req.user_kind = UserKind::General;
        req.structure = Some(example_structure(UserKind::General));
        let s = run_over(&req, current.clone()).unwrap();
        assert_eq!(s.root_folder, current.root_folder);
        assert!(root.join("문서").is_dir()); // 없던 폴더는 만들고
        assert!(root.join("평가").is_dir()); // 있던 폴더와
        assert_eq!(fs::read(root.join("업무").join("a.hwp")).unwrap(), b"data"); // 파일은 그대로
        assert_eq!(s.pinned_favorites, current.pinned_favorites); // 정리 폴더가 같으면 자주 쓰는 폴더도
        assert!(!s.search_whole_pc); // 다른 설정도 그대로
        assert_eq!((s.first_term_month, s.second_term_month), (1, 7)); // 쓰는 곳이 바뀌면 학기 달만

        // 쓰는 곳이 같으면 학기 달은 사용자가 고친 그대로, 폴더 구성 없이 쓰던 폴더면 아무것도 안 만듦
        let req = request(&root, FolderPreset::Existing, watch.clone());
        let s = run_over(&req, current.clone()).unwrap();
        assert_eq!(s.first_term_month, 2);

        // 정리 폴더를 바꾸면 직접 고른 자주 쓰는 폴더만 비운다
        let other = dir.path().join("새 위치");
        fs::create_dir_all(&other).unwrap();
        let s = run_over(&request(&other, FolderPreset::Empty, watch), current).unwrap();
        assert!(s.pinned_favorites.is_empty());
        assert!(other.join(ROOT_FOLDER_NAME).is_dir());
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
    fn watched_folder_can_be_the_root_as_is() {
        // 바탕화면 그대로 쓰기: 지켜볼 폴더를 정리 폴더로 그대로 쓴다(안의 폴더로 정리)
        let dir = tempfile::tempdir().unwrap();
        let desk = downloads(dir.path());
        fs::create_dir_all(Path::new(&desk).join("업무")).unwrap();
        let s = run(&request(
            Path::new(&desk),
            FolderPreset::Existing,
            vec![desk.clone()],
        ))
        .unwrap();
        assert_eq!(s.root_folder.as_deref(), Some(desk.as_str()));
        assert!(Path::new(&desk).join("업무").is_dir());
        // 지켜볼 폴더가 없으면 안 됨
        assert!(run(&request(Path::new(&desk), FolderPreset::Existing, vec![])).is_err());
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
