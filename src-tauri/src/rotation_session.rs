//! PDF rotation staging and parent-owned publication; no Tauri dependency.
//!
//! The worker never receives the final destination. A successful response is
//! only a staged candidate, independently read and hashed by the parent. The
//! current session is checked again while holding the publication fence.
//! A later external change to the source path does not invalidate an already
//! verified snapshot: the new copy still represents those captured bytes.
//! This does not lock the source or claim atomicity against hostile filesystem
//! changes. The source is never opened for writing.
use crate::{document_session::SessionState, worker};
use rust_wps::{PdfSourceIdentity, pdf_rotation::Selection};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

const STAGED_FILE_NAME: &str = "rotated.pdf";
const MAX_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StagedRotation {
    pub file_name: String,
    pub bytes: u64,
    pub sha256: [u8; 32],
    pub page: u32,
    pub pages: u32,
    pub rotation: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedRotation {
    pub path: String,
    pub page: u32,
    pub pages: u32,
    pub rotation: i32,
}

fn target_rotation(selection: &Selection) -> Result<i32, String> {
    if selection.page == 0
        || selection.page > selection.pages
        || selection.pages > 1000
        || selection.object_id == 0
        || selection.generation != 0
        || ![0, 90, 180, 270].contains(&selection.rotation)
    {
        return Err("PDF 页面选择无效，请重新打开所需页面".into());
    }
    Ok((selection.rotation + 90) % 360)
}

/// Called only inside the isolated worker. The directory is created and owned
/// by its parent, and the only file this operation can create has a fixed name.
pub fn stage_rotation(
    path: &Path,
    identity: &PdfSourceIdentity,
    selection: &Selection,
    staging_directory: &Path,
) -> Result<StagedRotation, String> {
    let rotation = target_rotation(selection)?;
    if !fs::symlink_metadata(staging_directory)
        .map_err(|e| format!("PDF 临时目录不可用：{e}"))?
        .is_dir()
    {
        return Err("PDF 临时目录无效".into());
    }
    let source = rust_wps::read_verified_pdf(path, identity).map_err(|e| format!("{e:#}"))?;
    let bytes = rust_wps::pdf_rotation::rotate(&source, selection).map_err(|e| format!("{e:#}"))?;
    if !(5..=MAX_BYTES).contains(&(bytes.len() as u64)) || !bytes.starts_with(&source) {
        return Err("PDF 旋转候选未保留原文件字节或超出大小上限".into());
    }
    let identity = rust_wps::pdf_rotation::identity(&bytes);
    let candidate_path = staging_directory.join(STAGED_FILE_NAME);
    let mut candidate = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(candidate_path)
        .map_err(|e| format!("无法新建 PDF 临时文件：{e}"))?;
    candidate
        .write_all(&bytes)
        .and_then(|()| candidate.sync_all())
        .map_err(|e| format!("无法写入 PDF 临时文件：{e}"))?;
    Ok(StagedRotation {
        file_name: STAGED_FILE_NAME.into(),
        bytes: identity.bytes,
        sha256: identity.sha256,
        page: selection.page,
        pages: selection.pages,
        rotation,
    })
}

/// The dialog is supplied by the thin native adapter. Cancellation does no
/// worker work and creates no temporary or final file. A newer committed
/// document invalidates the operation; a cancelled newer open does not.
pub fn rotate_current(
    session: &Mutex<SessionState>,
    token: &str,
    selection: Selection,
    pick_output: impl FnOnce(&Path) -> Option<PathBuf>,
) -> Result<Option<SavedRotation>, String> {
    rotate_current_with(session, token, selection, pick_output, worker::run, || {})
}

fn rotate_current_with(
    session: &Mutex<SessionState>,
    token: &str,
    selection: Selection,
    pick_output: impl FnOnce(&Path) -> Option<PathBuf>,
    run: impl FnOnce(worker::Request) -> Result<worker::Response, String>,
    before_publish: impl FnOnce(),
) -> Result<Option<SavedRotation>, String> {
    let rotation = target_rotation(&selection)?;
    let source = session
        .lock()
        .map_err(|e| e.to_string())?
        .pdf_source(token)?;
    let Some(output) = pick_output(&source.path) else {
        return Ok(None);
    };
    session
        .lock()
        .map_err(|e| e.to_string())?
        .ensure_current(token)?;
    let output = destination(&source.path, &output)?;
    let parent = output.parent().ok_or("PDF 输出目录无效")?;
    let staging = tempfile::Builder::new()
        .prefix(".rust-wps-rotation-")
        .tempdir_in(parent)
        .map_err(|e| format!("无法创建 PDF 临时目录：{e}"))?;
    let expected = selection.clone();
    let source_identity = source.identity.clone();
    let response = run(worker::Request::RotatePdf {
        path: source.path,
        identity: source.identity,
        selection,
        staging_directory: staging.path().to_path_buf(),
    })?;
    session
        .lock()
        .map_err(|e| e.to_string())?
        .ensure_current(token)?;
    let worker::Response::PdfRotated { candidate } = response else {
        return Err("PDF 旋转进程返回了错误类型".into());
    };
    let bytes = read_candidate(staging.path(), &candidate, &expected, &source_identity)?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| format!("无法准备 PDF 输出：{e}"))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|e| format!("无法准备 PDF 输出：{e}"))?;
    drop(bytes);
    before_publish();
    // All reads, parsing, hashing and syncing occurred outside the lock. Hold
    // it across the final no-clobber operation so a committed document change
    // cannot slip between ownership validation and publication.
    let state = session.lock().map_err(|e| e.to_string())?;
    state.ensure_current(token)?;
    temporary
        .persist_noclobber(&output)
        .map_err(|e| format!("无法另存 PDF；不会覆盖已有文件：{}", e.error))?;
    drop(state);
    Ok(Some(SavedRotation {
        path: output.to_string_lossy().into(),
        page: expected.page,
        pages: expected.pages,
        rotation,
    }))
}

fn destination(source: &Path, output: &Path) -> Result<PathBuf, String> {
    if !output
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    {
        return Err("请使用 .pdf 扩展名".into());
    }
    let name = output.file_name().ok_or("PDF 输出文件名无效")?;
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|e| format!("PDF 输出目录不可用：{e}"))?;
    let output = parent.join(name);
    let source =
        fs::canonicalize(source).map_err(|e| format!("PDF 源文件无法验证，请重新打开文档：{e}"))?;
    if output == source {
        return Err("PDF 旋转只能另存新文件，不能覆盖原文件".into());
    }
    match fs::symlink_metadata(&output) {
        Ok(_) => Err("PDF 输出已存在，请选择新的文件名；不会覆盖已有文件".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(output),
        Err(error) => Err(format!("无法检查 PDF 输出路径：{error}")),
    }
}

fn read_candidate(
    staging: &Path,
    candidate: &StagedRotation,
    expected: &Selection,
    source_identity: &PdfSourceIdentity,
) -> Result<Vec<u8>, String> {
    if candidate.file_name != STAGED_FILE_NAME
        || !(5..=MAX_BYTES).contains(&candidate.bytes)
        || candidate.page != expected.page
        || candidate.pages != expected.pages
        || candidate.rotation != target_rotation(expected)?
    {
        return Err("PDF 旋转进程返回了不匹配的候选身份".into());
    }
    let path = staging.join(STAGED_FILE_NAME);
    if !fs::symlink_metadata(&path)
        .map_err(|e| format!("PDF 临时文件无法读取：{e}"))?
        .is_file()
    {
        return Err("PDF 临时文件必须是普通文件".into());
    }
    let identity = PdfSourceIdentity {
        bytes: candidate.bytes,
        sha256: candidate.sha256,
    };
    let bytes = rust_wps::read_verified_pdf(&path, &identity)
        .map_err(|e| format!("PDF 旋转候选校验失败：{e:#}"))?;
    let prefix_length = usize::try_from(source_identity.bytes).map_err(|_| "PDF 原文件长度无效")?;
    if bytes.len() <= prefix_length
        || rust_wps::pdf_rotation::identity(&bytes[..prefix_length]) != *source_identity
    {
        return Err("PDF 旋转候选未保留已选择的原文件字节".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OpenedDocument;
    use rust_wps::Inspection;

    const SOURCE: &[u8] = b"%PDF-source-byte-identity";
    const CANDIDATE: &[u8] = b"%PDF-source-byte-identity\nappended-rotation";

    fn selection() -> Selection {
        Selection {
            page: 2,
            pages: 3,
            object_id: 7,
            generation: 0,
            rotation: 270,
        }
    }

    fn commit(session: &Mutex<SessionState>, path: &Path) -> String {
        let identity = rust_wps::pdf_rotation::identity(SOURCE);
        let opened = OpenedDocument {
            session_token: None,
            name: "source.pdf".into(),
            path: path.to_string_lossy().into(),
            inspection: Inspection {
                format: "pdf".into(),
                backend: "test transport, not a parsed PDF".into(),
                bytes: identity.bytes,
                elapsed_ms: 0,
                text: "unchanged text".into(),
                warnings: vec![],
                text_error: None,
            },
            authored: None,
            workbook: None,
            workbook_model: None,
            presentation: None,
            presentation_model: None,
        };
        let mut state = session.lock().unwrap();
        let ticket = state.reserve().unwrap();
        state
            .commit_open(ticket, path.into(), &opened, Some(identity))
            .unwrap()
    }

    struct Fixture {
        directory: tempfile::TempDir,
        source: PathBuf,
        output: PathBuf,
        session: Mutex<SessionState>,
        token: String,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("source.pdf");
            let output = directory.path().join("new.pdf");
            fs::write(&source, SOURCE).unwrap();
            let session = Mutex::new(SessionState::default());
            let token = commit(&session, &source);
            Self {
                directory,
                source,
                output,
                session,
                token,
            }
        }

        fn assert_clean(&self) {
            assert_eq!(fs::read(&self.source).unwrap(), SOURCE);
            assert!(!self.output.exists());
            let names: Vec<_> = fs::read_dir(self.directory.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            assert_eq!(names, [std::ffi::OsString::from("source.pdf")]);
        }
    }

    // These are transport/publication doubles, not a substitute for the core
    // parser or independent genuine-file rendering tests.
    fn staged(request: worker::Request) -> StagedRotation {
        let worker::Request::RotatePdf {
            path,
            identity,
            selection: requested,
            staging_directory,
        } = request
        else {
            panic!("Wrong request type")
        };
        assert_eq!(requested, selection());
        assert_eq!(
            rust_wps::read_verified_pdf(&path, &identity).unwrap(),
            SOURCE
        );
        assert!(staging_directory.is_dir());
        fs::write(staging_directory.join(STAGED_FILE_NAME), CANDIDATE).unwrap();
        let identity = rust_wps::pdf_rotation::identity(CANDIDATE);
        StagedRotation {
            file_name: STAGED_FILE_NAME.into(),
            bytes: identity.bytes,
            sha256: identity.sha256,
            page: requested.page,
            pages: requested.pages,
            rotation: 0,
        }
    }

    fn successful_worker(request: worker::Request) -> Result<worker::Response, String> {
        Ok(worker::Response::PdfRotated {
            candidate: staged(request),
        })
    }

    #[test]
    fn cancelled_dialog_does_not_start_worker_or_create_files() {
        let fixture = Fixture::new();
        let saved = rotate_current_with(
            &fixture.session,
            &fixture.token,
            selection(),
            |_| None,
            |_| panic!("Cancelled dialog must not start worker"),
            || panic!("Cancelled dialog must not publish"),
        )
        .unwrap();
        assert!(saved.is_none());
        fixture.assert_clean();
        fixture
            .session
            .lock()
            .unwrap()
            .ensure_current(&fixture.token)
            .unwrap();
    }

    #[test]
    fn stale_entry_is_rejected_before_dialog() {
        let fixture = Fixture::new();
        let result = rotate_current_with(
            &fixture.session,
            "obsolete",
            selection(),
            |_| panic!("Stale request must not open dialog"),
            |_| panic!("Stale request must not start worker"),
            || {},
        );
        assert!(result.unwrap_err().contains("会话"));
        fixture.assert_clean();
    }

    #[test]
    fn a_new_commit_during_dialog_rejects_before_worker() {
        let fixture = Fixture::new();
        let result = rotate_current_with(
            &fixture.session,
            &fixture.token,
            selection(),
            |_| {
                commit(&fixture.session, &fixture.source);
                Some(fixture.output.clone())
            },
            |_| panic!("Superseded dialog must not start worker"),
            || {},
        );
        assert!(result.unwrap_err().contains("会话"));
        fixture.assert_clean();
    }

    #[test]
    fn a_new_commit_during_worker_removes_staged_candidate_without_publish() {
        let fixture = Fixture::new();
        let result = rotate_current_with(
            &fixture.session,
            &fixture.token,
            selection(),
            |_| Some(fixture.output.clone()),
            |request| {
                let response = successful_worker(request)?;
                commit(&fixture.session, &fixture.source);
                Ok(response)
            },
            || panic!("Superseded worker must not prepare publication"),
        );
        assert!(result.unwrap_err().contains("会话"));
        fixture.assert_clean();
    }

    #[test]
    fn final_session_fence_rejects_after_candidate_read_and_sync() {
        let fixture = Fixture::new();
        let result = rotate_current_with(
            &fixture.session,
            &fixture.token,
            selection(),
            |_| Some(fixture.output.clone()),
            successful_worker,
            || {
                assert!(fixture.session.try_lock().is_ok());
                commit(&fixture.session, &fixture.source);
            },
        );
        assert!(result.unwrap_err().contains("会话"));
        fixture.assert_clean();
    }

    #[test]
    fn publication_preserves_current_source_and_does_not_hold_lock_during_work() {
        let fixture = Fixture::new();
        let saved = rotate_current_with(
            &fixture.session,
            &fixture.token,
            selection(),
            |path| {
                assert_eq!(path, fixture.source);
                assert!(fixture.session.try_lock().is_ok());
                Some(fixture.output.clone())
            },
            |request| {
                assert!(fixture.session.try_lock().is_ok());
                successful_worker(request)
            },
            || assert!(fixture.session.try_lock().is_ok()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(saved.path, fixture.output.to_string_lossy());
        assert_eq!((saved.page, saved.pages, saved.rotation), (2, 3, 0));
        assert_eq!(fs::read(&fixture.output).unwrap(), CANDIDATE);
        assert_eq!(fs::read(&fixture.source).unwrap(), SOURCE);
        let state = fixture.session.lock().unwrap();
        state.ensure_current(&fixture.token).unwrap();
        assert_eq!(state.current_path().unwrap(), fixture.source);
        assert_eq!(fs::read_dir(fixture.directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn cancelled_newer_open_keeps_committed_source_eligible_for_publication() {
        let fixture = Fixture::new();
        let saved = rotate_current_with(
            &fixture.session,
            &fixture.token,
            selection(),
            |_| Some(fixture.output.clone()),
            |request| {
                fixture.session.lock().unwrap().reserve().unwrap();
                successful_worker(request)
            },
            || {},
        )
        .unwrap();
        assert!(saved.is_some());
        fixture
            .session
            .lock()
            .unwrap()
            .ensure_current(&fixture.token)
            .unwrap();
    }

    #[test]
    fn worker_failure_cleans_partial_candidate_and_preserves_source() {
        let fixture = Fixture::new();
        for message in ["timeout", "worker crashed", "unsupported source"] {
            let result = rotate_current_with(
                &fixture.session,
                &fixture.token,
                selection(),
                |_| Some(fixture.output.clone()),
                |request| {
                    let _ = staged(request);
                    Err(message.into())
                },
                || panic!("Failed worker must not publish"),
            );
            assert_eq!(result.unwrap_err(), message);
            fixture.assert_clean();
        }
    }

    #[test]
    fn wrong_response_type_is_not_a_saved_pdf() {
        let fixture = Fixture::new();
        let result = rotate_current_with(
            &fixture.session,
            &fixture.token,
            selection(),
            |_| Some(fixture.output.clone()),
            |_| {
                Ok(worker::Response::Error {
                    message: "unexpected envelope".into(),
                })
            },
            || {},
        );
        assert!(result.unwrap_err().contains("错误类型"));
        fixture.assert_clean();
    }

    #[test]
    fn candidate_metadata_must_match_the_captured_selection_and_fixed_path() {
        for field in ["file_name", "bytes", "sha256", "page", "pages", "rotation"] {
            let fixture = Fixture::new();
            let result = rotate_current_with(
                &fixture.session,
                &fixture.token,
                selection(),
                |_| Some(fixture.output.clone()),
                |request| {
                    let mut candidate = staged(request);
                    match field {
                        "file_name" => candidate.file_name = "../source.pdf".into(),
                        "bytes" => candidate.bytes += 1,
                        "sha256" => candidate.sha256[0] ^= 1,
                        "page" => candidate.page += 1,
                        "pages" => candidate.pages += 1,
                        "rotation" => candidate.rotation = 90,
                        _ => unreachable!(),
                    }
                    Ok(worker::Response::PdfRotated { candidate })
                },
                || panic!("Mismatched candidate must not publish"),
            );
            assert!(result.is_err(), "{field}");
            fixture.assert_clean();
        }
    }

    #[test]
    fn self_consistent_candidate_for_wrong_source_is_rejected() {
        let fixture = Fixture::new();
        let result = rotate_current_with(
            &fixture.session,
            &fixture.token,
            selection(),
            |_| Some(fixture.output.clone()),
            |request| {
                let worker::Request::RotatePdf {
                    staging_directory, ..
                } = &request
                else {
                    unreachable!()
                };
                let staged_path = staging_directory.join(STAGED_FILE_NAME);
                let mut candidate = staged(request);
                let wrong = b"%PDF-another-source-with-self-consistent-digest";
                fs::write(staged_path, wrong).unwrap();
                let identity = rust_wps::pdf_rotation::identity(wrong);
                candidate.bytes = identity.bytes;
                candidate.sha256 = identity.sha256;
                Ok(worker::Response::PdfRotated { candidate })
            },
            || {},
        );
        assert!(result.unwrap_err().contains("原文件字节"));
        fixture.assert_clean();
    }

    #[test]
    fn candidate_wire_rejects_unknown_fields_and_wrong_digest_width() {
        let candidate = StagedRotation {
            file_name: STAGED_FILE_NAME.into(),
            bytes: 10,
            sha256: [0; 32],
            page: 2,
            pages: 3,
            rotation: 0,
        };
        let original = serde_json::to_value(candidate).unwrap();
        let mut bad = original.clone();
        bad["sha256"] = serde_json::json!(vec![0; 31]);
        assert!(serde_json::from_value::<StagedRotation>(bad).is_err());
        let mut bad = original;
        bad["output"] = serde_json::json!("source.pdf");
        assert!(serde_json::from_value::<StagedRotation>(bad).is_err());
    }

    #[test]
    fn invalid_selection_is_rejected_before_dialog() {
        for field in ["page", "pages", "object", "generation", "rotation"] {
            let fixture = Fixture::new();
            let mut selected = selection();
            match field {
                "page" => selected.page = 0,
                "pages" => selected.pages = 1001,
                "object" => selected.object_id = 0,
                "generation" => selected.generation = 1,
                "rotation" => selected.rotation = i32::MAX,
                _ => unreachable!(),
            }
            assert!(
                rotate_current_with(
                    &fixture.session,
                    &fixture.token,
                    selected,
                    |_| panic!("Invalid selection must not open dialog"),
                    |_| panic!("Invalid selection must not start worker"),
                    || {},
                )
                .is_err()
            );
            fixture.assert_clean();
        }
    }

    #[test]
    fn original_and_existing_destinations_are_rejected_before_worker() {
        let fixture = Fixture::new();
        fs::write(&fixture.output, b"existing destination").unwrap();
        for path in [&fixture.source, &fixture.output] {
            assert!(
                rotate_current_with(
                    &fixture.session,
                    &fixture.token,
                    selection(),
                    |_| Some(path.clone()),
                    |_| panic!("Existing destination must not start worker"),
                    || {},
                )
                .is_err()
            );
        }
        assert_eq!(fs::read(&fixture.source).unwrap(), SOURCE);
        assert_eq!(fs::read(&fixture.output).unwrap(), b"existing destination");
        assert_eq!(fs::read_dir(fixture.directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn destination_created_during_worker_is_never_overwritten() {
        let fixture = Fixture::new();
        let result = rotate_current_with(
            &fixture.session,
            &fixture.token,
            selection(),
            |_| Some(fixture.output.clone()),
            |request| {
                let response = successful_worker(request)?;
                fs::write(&fixture.output, b"concurrent destination").unwrap();
                Ok(response)
            },
            || {},
        );
        assert!(result.unwrap_err().contains("不会覆盖"));
        assert_eq!(
            fs::read(&fixture.output).unwrap(),
            b"concurrent destination"
        );
        assert_eq!(fs::read(&fixture.source).unwrap(), SOURCE);
        assert_eq!(fs::read_dir(fixture.directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn invalid_extension_and_unavailable_output_directory_leave_no_files() {
        let fixture = Fixture::new();
        for output in [
            fixture.output.with_extension("docx"),
            fixture.directory.path().join("missing/new.pdf"),
        ] {
            assert!(
                rotate_current_with(
                    &fixture.session,
                    &fixture.token,
                    selection(),
                    |_| Some(output),
                    |_| panic!("Invalid destination must not start worker"),
                    || {},
                )
                .is_err()
            );
            fixture.assert_clean();
        }
    }

    #[test]
    fn changed_source_during_dialog_is_rejected_before_editing() {
        let fixture = Fixture::new();
        let external = b"%PDF-external-replacement";
        let result = rotate_current_with(
            &fixture.session,
            &fixture.token,
            selection(),
            |_| {
                fs::write(&fixture.source, external).unwrap();
                Some(fixture.output.clone())
            },
            |request| {
                let worker::Request::RotatePdf {
                    path,
                    identity,
                    selection,
                    staging_directory,
                } = request
                else {
                    unreachable!()
                };
                stage_rotation(&path, &identity, &selection, &staging_directory)
                    .map(|candidate| worker::Response::PdfRotated { candidate })
            },
            || panic!("Changed source must not publish"),
        );
        assert!(result.unwrap_err().contains("已改变"));
        assert_eq!(fs::read(&fixture.source).unwrap(), external);
        assert!(!fixture.output.exists());
        assert_eq!(fs::read_dir(fixture.directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn later_external_source_change_does_not_replace_the_captured_candidate() {
        let fixture = Fixture::new();
        let external = b"%PDF-a-later-external-change";
        let saved = rotate_current_with(
            &fixture.session,
            &fixture.token,
            selection(),
            |_| Some(fixture.output.clone()),
            |request| {
                let response = successful_worker(request)?;
                fs::write(&fixture.source, external).unwrap();
                Ok(response)
            },
            || {},
        )
        .unwrap();
        assert!(saved.is_some());
        assert_eq!(fs::read(&fixture.output).unwrap(), CANDIDATE);
        assert_eq!(fs::read(&fixture.source).unwrap(), external);
        assert!(
            fixture
                .session
                .lock()
                .unwrap()
                .pdf_source(&fixture.token)
                .unwrap()
                .read()
                .is_err()
        );
        assert_eq!(fs::read_dir(fixture.directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn existing_hardlink_to_original_is_never_overwritten() {
        let fixture = Fixture::new();
        fs::hard_link(&fixture.source, &fixture.output).unwrap();
        assert!(
            rotate_current_with(
                &fixture.session,
                &fixture.token,
                selection(),
                |_| Some(fixture.output.clone()),
                |_| panic!("Hardlink target must not start worker"),
                || {},
            )
            .is_err()
        );
        assert_eq!(fs::read(&fixture.source).unwrap(), SOURCE);
        assert_eq!(fs::read(&fixture.output).unwrap(), SOURCE);
        assert_eq!(fs::read_dir(fixture.directory.path()).unwrap().count(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn source_alias_and_staged_symlink_are_rejected() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let alias = fixture.directory.path().join("alias.pdf");
        symlink(&fixture.source, &alias).unwrap();
        assert!(destination(&fixture.source, &alias).is_err());
        fs::remove_file(alias).unwrap();
        let result = rotate_current_with(
            &fixture.session,
            &fixture.token,
            selection(),
            |_| Some(fixture.output.clone()),
            |request| {
                let worker::Request::RotatePdf {
                    staging_directory, ..
                } = &request
                else {
                    unreachable!()
                };
                let staged_path = staging_directory.join(STAGED_FILE_NAME);
                let candidate = staged(request);
                fs::remove_file(&staged_path).unwrap();
                symlink(&fixture.source, staged_path).unwrap();
                Ok(worker::Response::PdfRotated { candidate })
            },
            || {},
        );
        assert!(result.unwrap_err().contains("普通文件"));
        fixture.assert_clean();
    }
}
