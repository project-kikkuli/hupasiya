use hupasiya::config::Config;
use hupasiya::hn_client::HnClient;
use hupasiya::models::{AgentType, Session};
use hupasiya::session::SessionManager;
use std::sync::{Arc, Barrier};

fn manager(config: &Config) -> SessionManager {
    SessionManager::with_client(config.clone(), HnClient::with_command("unused-hn".into())).unwrap()
}

fn session(name: &str) -> Session {
    Session::new(
        name.into(),
        AgentType::Feature,
        name.into(),
        std::path::PathBuf::from("/synthetic/worktree"),
        name.into(),
        "main".into(),
        "synthetic-repo".into(),
        "git".into(),
    )
}

#[test]
fn concurrent_child_links_preserve_every_relationship() {
    // Separate managers and file handles model independent hp commands.
    // Repeat a bounded four-worker start to exercise overlapping file updates.
    for round in 0..16 {
        let temporary = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.hp.sessions.metadata_dir = temporary.path().join("sessions");
        let store = manager(&config);
        store.save_session(&session("parent")).unwrap();
        for i in 0..4 {
            store.save_session(&session(&format!("child-{i}"))).unwrap();
        }
        let start = Arc::new(Barrier::new(4));
        let workers: Vec<_> = (0..4)
            .map(|i| {
                let config = config.clone();
                let start = start.clone();
                std::thread::spawn(move || {
                    let store = manager(&config);
                    start.wait();
                    store.link_parent_child("parent", &format!("child-{i}"))
                })
            })
            .collect();
        let outcomes: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert!(
            outcomes.iter().all(Result::is_ok),
            "round {round}: {outcomes:?}"
        );
        let parent = store.load_session("parent").unwrap();
        assert_eq!(
            parent.children.len(),
            4,
            "round {round}: {:?}",
            parent.children
        );
        for i in 0..4 {
            let name = format!("child-{i}");
            assert!(parent.children.contains(&name));
            assert_eq!(
                store.load_session(&name).unwrap().parent.as_deref(),
                Some("parent")
            );
        }
    }
}

#[test]
fn listing_during_writes_never_silently_loses_a_session() {
    let temporary = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.hp.sessions.metadata_dir = temporary.path().join("sessions");
    let store = manager(&config);
    let mut record = session("active");
    record.tags = (0..500)
        .map(|i| format!("synthetic-tag-{i}-{}", "x".repeat(100)))
        .collect();
    store.save_session(&record).unwrap();
    let start = Arc::new(Barrier::new(2));
    let writer_start = start.clone();
    let writer_config = config.clone();
    let writer = std::thread::spawn(move || {
        let store = manager(&writer_config);
        writer_start.wait();
        for _ in 0..32 {
            store.save_session(&record).unwrap();
        }
    });
    start.wait();
    let mut missing = 0;
    for _ in 0..128 {
        if store.list_sessions().unwrap().len() != 1 {
            missing += 1;
        }
    }
    writer.join().unwrap();
    assert_eq!(
        missing, 0,
        "list silently omitted an existing session during {missing} reads"
    );
}

#[test]
fn child_link_process_worker() {
    let Some(root) = std::env::var_os("HP_STORAGE_TEST_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let worker = std::env::var("HP_STORAGE_TEST_WORKER").unwrap();
    let mut config = Config::default();
    config.hp.sessions.metadata_dir = root.join("sessions");
    let store = manager(&config);
    std::fs::write(root.join(format!("ready-{worker}")), "ready").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while !root.join("start").exists() {
        assert!(std::time::Instant::now() < deadline, "start gate timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    for round in 0..8 {
        store
            .link_parent_child(
                &format!("parent-{round}"),
                &format!("child-{round}-{worker}"),
            )
            .unwrap();
    }
}

#[test]
fn independent_processes_preserve_child_links() {
    let temporary = tempfile::tempdir().unwrap();
    let mut config = Config::default();
    config.hp.sessions.metadata_dir = temporary.path().join("sessions");
    let store = manager(&config);
    for round in 0..8 {
        store
            .save_session(&session(&format!("parent-{round}")))
            .unwrap();
        for worker in 0..4 {
            store
                .save_session(&session(&format!("child-{round}-{worker}")))
                .unwrap();
        }
    }
    let mut workers: Vec<_> = (0..4)
        .map(|worker| {
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "child_link_process_worker", "--nocapture"])
                .env("HP_STORAGE_TEST_ROOT", temporary.path())
                .env("HP_STORAGE_TEST_WORKER", worker.to_string())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !(0..4).all(|worker| temporary.path().join(format!("ready-{worker}")).exists()) {
        assert!(
            std::time::Instant::now() < deadline,
            "workers did not reach start gate"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    std::fs::write(temporary.path().join("start"), "start").unwrap();
    let mut timed_out = false;
    for worker in &mut workers {
        while worker.try_wait().unwrap().is_none() {
            if std::time::Instant::now() >= deadline {
                worker.kill().unwrap();
                timed_out = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    let outputs: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.wait_with_output().unwrap())
        .collect();
    assert!(!timed_out, "session update workers timed out");
    for output in outputs {
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for round in 0..8 {
        let parent = store.load_session(&format!("parent-{round}")).unwrap();
        assert_eq!(parent.children.len(), 4);
        for worker in 0..4 {
            let child = format!("child-{round}-{worker}");
            assert!(parent.children.contains(&child));
            assert_eq!(
                store.load_session(&child).unwrap().parent,
                Some(parent.name.clone())
            );
        }
    }
}
