//! Document-selection ordering and PDF delivery identity. No Tauri dependency.
use crate::OpenedDocument;
use rust_wps::{Inspection, PdfSourceIdentity};
use std::{path::PathBuf, sync::Mutex};

const STALE: &str = "文档会话已更新，请重新打开所需文档";
const BAD_IDENTITY: &str = "解析进程返回的 PDF 源身份无效，请重新打开文档";

pub fn validate_identity(
    inspection: &Inspection,
    identity: Option<&PdfSourceIdentity>,
) -> Result<(), String> {
    match (inspection.format.as_str(), identity) {
        ("pdf", Some(identity))
            if identity.bytes == inspection.bytes
                && (5..=64 * 1024 * 1024).contains(&identity.bytes) =>
        {
            Ok(())
        }
        ("pdf", _) => Err(BAD_IDENTITY.into()),
        (_, None) => Ok(()),
        (_, Some(_)) => Err(BAD_IDENTITY.into()),
    }
}

#[derive(Clone)]
struct Committed {
    path: PathBuf,
    token: String,
    pdf_identity: Option<PdfSourceIdentity>,
}

#[derive(Default)]
pub struct SessionState {
    latest_ticket: u64,
    current: Option<Committed>,
}

pub struct PdfSource {
    pub path: PathBuf,
    pub identity: PdfSourceIdentity,
}
impl PdfSource {
    pub fn read(&self) -> Result<Vec<u8>, String> {
        rust_wps::read_verified_pdf(&self.path, &self.identity).map_err(|e| format!("{e:#}"))
    }
}

/// Both ownership checks surround exactly one bounded verified read. The mutex
/// is released before I/O, so selecting another document can supersede a read.
pub fn read_pdf(session: &Mutex<SessionState>, token: &str) -> Result<Vec<u8>, String> {
    read_pdf_with(session, token, |source| source.read())
}

fn read_pdf_with(
    session: &Mutex<SessionState>,
    token: &str,
    read: impl FnOnce(PdfSource) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    let source = session
        .lock()
        .map_err(|e| e.to_string())?
        .pdf_source(token)?;
    let bytes = read(source)?;
    session
        .lock()
        .map_err(|e| e.to_string())?
        .ensure_current(token)?;
    Ok(bytes)
}

impl SessionState {
    // Reservations invalidate older pending selections, never the committed one.
    // Even cancel/failure keeps this ticket so older completions cannot resurrect.
    pub fn reserve(&mut self) -> Result<u64, String> {
        self.latest_ticket = self
            .latest_ticket
            .checked_add(1)
            .ok_or("文档会话编号已耗尽，请重启应用")?;
        Ok(self.latest_ticket)
    }
    pub fn current_path(&self) -> Option<PathBuf> {
        self.current.as_ref().map(|current| current.path.clone())
    }
    pub fn commit_open(
        &mut self,
        ticket: u64,
        path: PathBuf,
        opened: &OpenedDocument,
        identity: Option<PdfSourceIdentity>,
    ) -> Result<String, String> {
        validate_identity(&opened.inspection, identity.as_ref())?;
        if opened.path != path.to_string_lossy()
            || path
                .extension()
                .and_then(|s| s.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
                != Some(opened.inspection.format.as_str())
        {
            return Err("解析进程返回的文档路径或格式不匹配".into());
        }
        self.commit(ticket, path, identity)
    }
    pub fn commit_non_pdf(&mut self, ticket: u64, path: PathBuf) -> Result<String, String> {
        if path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
        {
            return Err(BAD_IDENTITY.into());
        }
        self.commit(ticket, path, None)
    }
    fn commit(
        &mut self,
        ticket: u64,
        path: PathBuf,
        pdf_identity: Option<PdfSourceIdentity>,
    ) -> Result<String, String> {
        if ticket == 0
            || ticket != self.latest_ticket
            || self
                .current
                .as_ref()
                .is_some_and(|current| current.token == ticket.to_string())
        {
            return Err(STALE.into());
        }
        let token = ticket.to_string();
        self.current = Some(Committed {
            path,
            token: token.clone(),
            pdf_identity,
        });
        Ok(token)
    }
    pub fn ensure_current(&self, token: &str) -> Result<(), String> {
        if !self
            .current
            .as_ref()
            .is_some_and(|current| current.token == token)
        {
            return Err(STALE.into());
        }
        Ok(())
    }
    pub fn pdf_source(&self, token: &str) -> Result<PdfSource, String> {
        self.ensure_current(token)?;
        let current = self.current.as_ref().ok_or(STALE)?;
        let identity = current.pdf_identity.clone().ok_or("当前文件不是 PDF")?;
        Ok(PdfSource {
            path: current.path.clone(),
            identity,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn opened(path: &str) -> OpenedDocument {
        OpenedDocument {
            session_token: None,
            name: path.into(),
            path: path.into(),
            inspection: Inspection {
                format: "pdf".into(),
                backend: "test".into(),
                bytes: 6,
                elapsed_ms: 0,
                text: "retained text".into(),
                warnings: vec![],
                text_error: None,
                pdf_actual_text: None,
            },
            authored: None,
            workbook: None,
            workbook_model: None,
            presentation: None,
            presentation_model: None,
        }
    }
    fn identity() -> PdfSourceIdentity {
        PdfSourceIdentity {
            bytes: 6,
            sha256: [5; 32],
        }
    }
    fn open(session: &mut SessionState, path: &str) -> String {
        let ticket = session.reserve().unwrap();
        session
            .commit_open(ticket, path.into(), &opened(path), Some(identity()))
            .unwrap()
    }

    #[test]
    fn cancelled_and_failed_new_attempts_preserve_committed_source_but_fence_old_pending_open() {
        let mut session = SessionState::default();
        let original = open(&mut session, "old.pdf");
        let pending = session.reserve().unwrap();
        let _cancelled = session.reserve().unwrap();
        assert!(
            session
                .commit_open(
                    pending,
                    "pending.pdf".into(),
                    &opened("pending.pdf"),
                    Some(identity())
                )
                .is_err()
        );
        session.pdf_source(&original).unwrap();
        assert_eq!(session.current_path().unwrap(), PathBuf::from("old.pdf"));
        let failure = session.reserve().unwrap();
        assert!(
            session
                .commit_open(failure, "bad.pdf".into(), &opened("bad.pdf"), None)
                .is_err()
        );
        session.pdf_source(&original).unwrap();
    }
    #[test]
    fn stale_delivery_rejected_before_io_and_after_concurrent_selection_without_locking_io() {
        use std::sync::{Arc, Barrier};
        let mut initial = SessionState::default();
        let old = open(&mut initial, "a.pdf");
        let session = Arc::new(Mutex::new(initial));
        assert!(
            read_pdf_with(&session, "obsolete", |_| panic!(
                "stale entry must not read"
            ))
            .is_err()
        );
        let started = Arc::new(Barrier::new(2));
        let resume = Arc::new(Barrier::new(2));
        let reader = {
            let session = Arc::clone(&session);
            let started = Arc::clone(&started);
            let resume = Arc::clone(&resume);
            std::thread::spawn(move || {
                read_pdf_with(&session, &old, |_| {
                    started.wait();
                    resume.wait();
                    Ok(vec![1, 2, 3])
                })
            })
        };
        started.wait();
        let mut state = session
            .try_lock()
            .expect("read must not hold session mutex");
        let new = open(&mut state, "b.pdf");
        drop(state);
        resume.wait();
        assert!(reader.join().unwrap().is_err());
        session.lock().unwrap().ensure_current(&new).unwrap();
    }
    #[test]
    fn delivery_returns_the_exact_verified_vector_without_reopening_or_copying() {
        let mut initial = SessionState::default();
        let token = open(&mut initial, "a.pdf");
        let session = Mutex::new(initial);
        let bytes = vec![1, 2, 3];
        let pointer = bytes.as_ptr();
        let returned = read_pdf_with(&session, &token, |_| Ok(bytes)).unwrap();
        assert_eq!(pointer, returned.as_ptr());
        assert_eq!(returned, [1, 2, 3]);
    }
    #[test]
    fn committed_ticket_cannot_be_reused_for_a_different_source() {
        let mut session = SessionState::default();
        let token = open(&mut session, "a.pdf");
        assert!(
            session
                .commit_open(
                    session.latest_ticket,
                    "b.pdf".into(),
                    &opened("b.pdf"),
                    Some(identity())
                )
                .is_err()
        );
        session.ensure_current(&token).unwrap();
        assert_eq!(session.current_path().unwrap(), PathBuf::from("a.pdf"));
    }
    #[test]
    fn newer_success_cannot_be_overwritten_by_older_open() {
        let mut session = SessionState::default();
        let a = session.reserve().unwrap();
        let b = session.reserve().unwrap();
        let token = session
            .commit_open(b, "b.pdf".into(), &opened("b.pdf"), Some(identity()))
            .unwrap();
        assert!(
            session
                .commit_open(a, "a.pdf".into(), &opened("a.pdf"), Some(identity()))
                .is_err()
        );
        session.ensure_current(&token).unwrap();
        assert_eq!(session.current_path().unwrap(), PathBuf::from("b.pdf"));
    }
    #[test]
    fn same_path_reopen_has_new_exact_token_and_rejects_old_read_at_both_fences() {
        let mut session = SessionState::default();
        let old = open(&mut session, "same.pdf");
        let _snapshot_before_io = session.pdf_source(&old).unwrap();
        let new = open(&mut session, "same.pdf");
        assert_ne!(old, new);
        assert!(session.pdf_source(&old).is_err());
        assert!(session.ensure_current(&old).is_err());
        session.pdf_source(&new).unwrap();
    }
    #[test]
    fn pending_selection_does_not_invalidate_committed_read() {
        let mut session = SessionState::default();
        let token = open(&mut session, "old.pdf");
        let _pending = session.reserve().unwrap();
        session.pdf_source(&token).unwrap();
        session.ensure_current(&token).unwrap();
    }
    #[test]
    fn tokens_above_javascript_precision_are_exact_and_overflow_does_not_wrap() {
        let mut session = SessionState {
            latest_ticket: 9_007_199_254_740_991,
            current: None,
        };
        assert_eq!(open(&mut session, "a.pdf"), "9007199254740992");
        let token = open(&mut session, "a.pdf");
        assert_eq!(token, "9007199254740993");
        for invalid in [
            "9007199254740992",
            "09007199254740993",
            "9.007199254740993e15",
            "",
            "0",
        ] {
            assert!(session.pdf_source(invalid).is_err());
        }
        session.latest_ticket = u64::MAX;
        assert!(session.reserve().is_err());
        session.ensure_current(&token).unwrap();
    }
    #[test]
    fn presentation_selection_is_fenced_and_clears_pdf_identity() {
        let mut session = SessionState::default();
        let old = open(&mut session, "a.pdf");
        let save = session.reserve().unwrap();
        let newer = open(&mut session, "b.pdf");
        assert!(session.commit_non_pdf(save, "saved.pptx".into()).is_err());
        session.pdf_source(&newer).unwrap();
        let save = session.reserve().unwrap();
        let token = session.commit_non_pdf(save, "saved.pptx".into()).unwrap();
        assert!(session.pdf_source(&old).is_err());
        assert!(session.pdf_source(&token).is_err());
        assert_eq!(session.current_path().unwrap(), PathBuf::from("saved.pptx"));
    }
    #[test]
    fn invalid_protocol_identity_and_disguised_formats_never_commit() {
        let mut session = SessionState::default();
        let original = open(&mut session, "old.pdf");
        for bytes in [0, 4, 7, 64 * 1024 * 1024 + 1] {
            let ticket = session.reserve().unwrap();
            let mut identity = identity();
            identity.bytes = bytes;
            assert!(
                session
                    .commit_open(ticket, "a.pdf".into(), &opened("a.pdf"), Some(identity))
                    .is_err()
            );
        }
        let ticket = session.reserve().unwrap();
        let mut wrong = opened("a.pdf");
        wrong.inspection.format = "docx".into();
        assert!(
            session
                .commit_open(ticket, "a.pdf".into(), &wrong, None)
                .is_err()
        );
        assert!(
            session
                .commit_open(
                    ticket,
                    "different.pdf".into(),
                    &opened("a.pdf"),
                    Some(identity())
                )
                .is_err()
        );
        session.ensure_current(&original).unwrap();
    }
    #[test]
    fn wire_identity_rejects_missing_null_bad_digest_and_lengths_before_commit() {
        let mut envelope = serde_json::json!({"result":"opened", "document":opened("a.pdf"), "pdf_identity":identity()});
        let attempt = |value: serde_json::Value| {
            let response = serde_json::from_value::<crate::worker::Response>(value)
                .map_err(|e| e.to_string())?;
            let crate::worker::Response::Opened {
                document,
                pdf_identity,
            } = response
            else {
                return Err("wrong kind".into());
            };
            let mut session = SessionState::default();
            let ticket = session.reserve()?;
            session.commit_open(ticket, "a.pdf".into(), &document, pdf_identity)
        };
        assert!(attempt(envelope.clone()).is_ok());
        for value in [
            serde_json::Value::Null,
            serde_json::json!({"bytes":6,"sha256":vec![5;31]}),
            serde_json::json!({"bytes":7,"sha256":vec![5;32]}),
            serde_json::json!({"bytes":6,"sha256":vec![256;32]}),
        ] {
            let mut bad = envelope.clone();
            bad["pdf_identity"] = value;
            assert!(attempt(bad).is_err());
        }
        envelope.as_object_mut().unwrap().remove("pdf_identity");
        assert!(attempt(envelope).is_err());
    }
}
