//! One stored copy of a body, held alive by whichever store still needs it.
use super::*;
use crate::providers::saved_answers::{KeptAnswers, body_digest};
use octocode_github::{ProviderAnswer, ProviderEvidence, SavedAnswer, SavedAnswers as _};
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;
use std::time::SystemTime;

const TOOL: ToolId = ToolId::GhSearchRepo;
const REQUEST: &str = "GET https://x/search";

struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("home"),
        }
    }
    fn answers_dir(&self) -> PathBuf {
        self.dir.path().join("answers")
    }
    fn continuations_dir(&self) -> PathBuf {
        self.dir.path().join("continuations")
    }
    fn answers(&self) -> KeptAnswers {
        KeptAnswers::new(Some(self.answers_dir()))
    }
    fn registry(&self) -> Registry {
        Registry::new(
            CacheConfig::derived(),
            Some(self.continuations_dir()),
            "policy".into(),
        )
        .linking_answers(Some(self.answers_dir()))
    }
}

/// Retain `observed` as a continuation of `access` and name its handle.
fn issue(registry: &Registry, access: u8, identity: &str, observed: ProviderEvidence) -> String {
    let mut pending = Pending::new([access; 32], Some(identity.into()));
    pending.own_evidence(TOOL, observed);
    let token = pending.insert(
        TOOL,
        json!({"keywords": [identity]}),
        SavedWork::ToolResume { index: Some(0) },
    );
    registry.retain(pending, &context()).expect("retained");
    token
}
fn replay(registry: &Registry, access: u8, token: &str) -> Option<ProviderEvidence> {
    let opened = registry.open(TOOL, token, [access; 32]).ok()?;
    registry.evidence(&opened).map(|(_, kept)| (*kept).clone())
}

fn answer(body: &str) -> ProviderAnswer {
    ProviderAnswer {
        request: REQUEST.into(),
        status: 200,
        headers: vec![("etag".into(), "\"e\"".into())],
        body: body.into(),
        next: None,
        not_found: None,
    }
}

fn saved(body: &str, observed_ms: u64) -> SavedAnswer {
    SavedAnswer {
        answer: answer(body),
        observed_ms,
        redirected: false,
    }
}

fn observed(answers: Vec<ProviderAnswer>) -> ProviderEvidence {
    ProviderEvidence {
        answers,
        notes: [("fact".to_owned(), "kept".to_owned())]
            .into_iter()
            .collect(),
    }
}

/// Every file under `dir` holding `text`.
fn holding(dir: &Path, text: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            std::fs::read(path).is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(text))
        })
        .collect()
}

/// The answer's and the evidence's copy are one file, its entries in the two
/// stores name it, and the evidence reads back byte for byte under the digest
/// the whole evidence always had.
#[test]
fn a_saved_body_and_its_evidence_are_one_file() {
    let home = Home::new();
    let body = "one stored body é\n";
    home.answers().save("scope", REQUEST, saved(body, 1));
    let registry = home.registry();
    let evidence = observed(vec![answer(body)]);
    let address = crate::digest::json_sha256(&evidence);
    let token = issue(&registry, 1, "one", evidence.clone());

    let in_answers = holding(&home.answers_dir(), body);
    let in_continuations = holding(&home.continuations_dir(), body);
    assert_eq!((in_answers.len(), in_continuations.len()), (1, 1));
    assert_eq!(
        std::fs::metadata(&in_answers[0]).expect("meta").ino(),
        std::fs::metadata(&in_continuations[0]).expect("meta").ino(),
    );
    let opened = registry.open(TOOL, &token, [1; 32]).expect("opened");
    let (digest, whole) = registry.evidence(&opened).expect("evidence");
    assert_eq!(*whole, evidence);
    assert_eq!(digest, address);
}

/// Evidence keeps its snapshot through everything that ends a saved answer:
/// losing access, a new body, eviction, and the whole store going away.
#[test]
fn evidence_outlives_whatever_ends_its_saved_answer() {
    type End = fn(&Home, &KeptAnswers);
    let ends: [End; 4] = [
        |_, answers| answers.forget("scope", REQUEST),
        |_, answers| answers.save("scope", REQUEST, saved("a newer body\n", 2)),
        |home, _| Disk::new(home.answers_dir()).prune(0),
        |home, _| std::fs::remove_dir_all(home.answers_dir()).expect("removed"),
    ];
    for end in ends {
        let home = Home::new();
        let answers = home.answers();
        answers.save("scope", REQUEST, saved("the snapshot\n", 1));
        let registry = home.registry();
        let evidence = observed(vec![answer("the snapshot\n")]);
        let token = issue(&registry, 1, "one", evidence.clone());
        end(&home, &answers);
        assert_ne!(
            answers.load("scope", REQUEST).map(|kept| kept.answer.body),
            Some("the snapshot\n".into()),
        );
        assert_eq!(replay(&registry, 1, &token), Some(evidence.clone()));
        // A process that was not there reads the same.
        assert_eq!(replay(&home.registry(), 1, &token), Some(evidence));
    }
}

/// A saved answer keeps its body when the continuation's link ages out, and
/// the continuation then has no evidence: never part of it, never other bytes.
#[test]
fn an_expired_link_ends_only_the_evidence() {
    let home = Home::new();
    let answers = home.answers();
    answers.save("scope", REQUEST, saved("shared\n", 1));
    let registry = home.registry();
    let token = issue(&registry, 1, "one", observed(vec![answer("shared\n")]));
    let link = holding(&home.continuations_dir(), "shared\n").remove(0);
    let aged = SystemTime::now() - crate::cache::CONTINUATION_IDLE - Duration::from_secs(1);
    std::fs::File::options()
        .write(true)
        .open(&link)
        .expect("link")
        .set_modified(aged)
        .expect("aged");
    Disk::new(home.continuations_dir()).prune(u64::MAX);
    assert!(!link.exists());
    assert!(replay(&registry, 1, &token).is_none());
    assert_eq!(
        answers.load("scope", REQUEST).expect("saved").answer.body,
        "shared\n"
    );
}

/// A file that no longer holds what its name says answers nothing, in either
/// store: a repeat asks again and a continuation has no evidence.
#[test]
fn a_changed_body_file_is_never_served() {
    let home = Home::new();
    let answers = home.answers();
    answers.save("scope", REQUEST, saved("the real bytes\n", 1));
    let registry = home.registry();
    let token = issue(
        &registry,
        1,
        "one",
        observed(vec![answer("the real bytes\n")]),
    );
    let file = holding(&home.answers_dir(), "the real bytes\n").remove(0);
    let bytes = std::fs::read(&file).expect("file");
    let changed = String::from_utf8(bytes)
        .expect("text")
        .replace("the real bytes", "the fake bytes");
    std::fs::write(&file, changed).expect("in place");
    assert!(replay(&registry, 1, &token).is_none());
    assert!(answers.load("scope", REQUEST).is_none());
}

/// An answer no saved answer holds (`no-store`, a partial page) stays in the
/// evidence entry, with evidence's own lifetime and nothing longer.
#[test]
fn a_body_the_answers_do_not_hold_stays_with_the_evidence() {
    let home = Home::new();
    let registry = home.registry();
    let evidence = observed(vec![answer("not kept\n")]);
    let token = issue(&registry, 1, "one", evidence.clone());
    assert!(holding(&home.answers_dir(), "not kept").is_empty());
    let files = holding(&home.continuations_dir(), "not kept");
    assert_eq!(files.len(), 1);
    assert_ne!(
        files[0].extension().and_then(|ext| ext.to_str()),
        Some("link")
    );
    assert_eq!(replay(&registry, 1, &token), Some(evidence));
}

/// A gate-held answer reaches no file in either store, as itself or as a
/// body; the process that holds it replays it, and another has none of it.
#[test]
fn a_held_body_reaches_neither_store_on_disk() {
    let home = Home::new();
    let secret = format!("token = \"ghp_{}\"\n", "a1B2c3D4e5F6".repeat(3));
    let manifest =
        "apiVersion: v1\nkind: Secret\ndata:\n  tls.crt: QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo=\n";
    let mut gated = answer(manifest);
    gated.request = "GET https://x/contents/k8s/secret.yaml?ref=main".into();
    let mut raw = answer(&secret);
    raw.request = "GET https://x/raw".into();
    let answers = home.answers();
    for held in [&raw, &gated] {
        let kept = SavedAnswer {
            answer: held.clone(),
            observed_ms: 1,
            redirected: false,
        };
        answers.save("scope", &held.request, kept);
        assert_eq!(
            answers.load("scope", &held.request).expect("held").answer,
            *held
        );
    }
    let mut clean = answer("clean\n");
    clean.request = "GET https://x/clean".into();
    let registry = home.registry();
    let token = issue(
        &registry,
        1,
        "one",
        observed(vec![clean.clone(), raw.clone(), gated.clone()]),
    );
    for dir in [home.answers_dir(), home.continuations_dir()] {
        for text in ["ghp_", "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo"] {
            assert!(
                holding(&dir, text).is_empty(),
                "{text} in {}",
                dir.display()
            );
        }
    }
    let whole = replay(&registry, 1, &token).expect("whole");
    assert_eq!(
        whole.answers,
        vec![clean.clone(), raw.clone(), gated.clone()]
    );
    let restarted = replay(&home.registry(), 1, &token).expect("sealed");
    assert_eq!(restarted.answers, vec![clean]);
    let restarted = home.answers();
    assert!(restarted.load("scope", &raw.request).is_none());
    assert!(restarted.load("scope", &gated.request).is_none());
}

/// Identical bytes under two credentials: one's saved answer answers neither
/// the other's repeat nor its evidence, its handle opens only for its own
/// access, and what one loses leaves the other's evidence whole.
#[test]
fn credentials_stay_apart_on_the_same_bytes() {
    let home = Home::new();
    let answers = home.answers();
    let body = "the same bytes\n";
    answers.save("scope-a", REQUEST, saved(body, 1));
    assert!(answers.load("scope-b", REQUEST).is_none());
    let registry = home.registry();
    let evidence = observed(vec![answer(body)]);
    let digest = crate::digest::json_sha256(&evidence);
    let token_a = issue(&registry, 1, "identity-a", evidence.clone());
    assert!(matches!(
        registry.open(TOOL, &token_a, [2; 32]),
        Err(Refusal::OtherAccess)
    ));
    let under_b = evidence::key("policy", Some("identity-b"), &digest);
    assert!(evidence::open(&registry.evidence, &registry.held_evidence, &under_b).is_none());

    let token_b = issue(&registry, 2, "identity-b", evidence.clone());
    answers.forget("scope-a", REQUEST);
    assert!(evidence::open(&registry.evidence, &registry.held_evidence, &under_b).is_some());
    assert_eq!(replay(&registry, 2, &token_b), Some(evidence.clone()));
    assert_eq!(replay(&registry, 1, &token_a), Some(evidence));
}

/// Confirming an answer (a 304) rewrites its metadata, not its body, and a
/// replaced body leaves the saved answers' directory at once.
#[test]
fn a_saved_body_is_written_once_and_leaves_with_its_answer() {
    let home = Home::new();
    let answers = home.answers();
    answers.save("scope", REQUEST, saved("first body\n", 1));
    let file = holding(&home.answers_dir(), "first body\n").remove(0);
    let inode = std::fs::metadata(&file).expect("meta").ino();
    answers.save("scope", REQUEST, saved("first body\n", 2));
    let kept = answers.load("scope", REQUEST).expect("kept");
    assert_eq!(
        (kept.observed_ms, kept.answer.body.as_str()),
        (2, "first body\n")
    );
    assert_eq!(std::fs::metadata(&file).expect("meta").ino(), inode);
    answers.save("scope", REQUEST, saved("second body\n", 3));
    assert!(holding(&home.answers_dir(), "first body\n").is_empty());
    answers.forget("scope", REQUEST);
    assert!(holding(&home.answers_dir(), "second body\n").is_empty());
}

/// In memory, an answer and its evidence hold one allocation, and evidence
/// keeps it after the answer is gone.
#[test]
fn in_memory_one_allocation_outlives_the_saved_answer() {
    let answers = KeptAnswers::new(None);
    let registry = Registry::new(CacheConfig::derived(), None, "policy".into());
    let origin = answer("one allocation\n");
    let at = origin.body.as_bytes().as_ptr();
    answers.save(
        "scope",
        REQUEST,
        SavedAnswer {
            answer: origin.clone(),
            observed_ms: 1,
            redirected: false,
        },
    );
    let loaded = answers.load("scope", REQUEST).expect("loaded");
    assert_eq!(loaded.answer.body.as_bytes().as_ptr(), at);
    assert_eq!(body_digest(&loaded.answer.body), body_digest(&origin.body));
    let token = issue(&registry, 1, "one", observed(vec![loaded.answer]));
    answers.forget("scope", REQUEST);
    let kept = replay(&registry, 1, &token).expect("evidence");
    assert_eq!(kept.answers[0].body.as_bytes().as_ptr(), at);
}
