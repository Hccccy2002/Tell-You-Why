//! Explicit manual acceptance only: uses an already configured provider and a scratch database.
use crate::{
    db::Database, providers::RestrictedHttpClient, secret_store::WindowsCredentialStore, AppState,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

#[test]
#[ignore = "requires explicit live-provider configuration and sends textbook excerpts; see RAG guide"]
fn rag_live_textbook_acceptance() {
    tauri::async_runtime::block_on(async {
        let snapshot = PathBuf::from(
            std::env::var_os("TELLWHY_RAG_TEST_PROFILE_DB")
                .expect("explicit profile snapshot required"),
        );
        let provider_id =
            std::env::var("TELLWHY_RAG_TEST_PROVIDER").expect("explicit provider required");
        let profiles = Database::new(snapshot).provider_profiles().unwrap();
        let profile = profiles
            .into_iter()
            .find(|p| {
                p.provider_id == provider_id && p.connection_verified && p.key_last4.is_some()
            })
            .expect("verified provider required");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let report_dir = root.join("tmp/rag-reports");
        let source: Value =
            serde_json::from_slice(&std::fs::read(report_dir.join("evidence-final.json")).unwrap())
                .unwrap();
        let database = Database::new(report_dir.join(format!("live-{}.db", uuid::Uuid::new_v4())));
        database.initialize().unwrap();
        database.save_provider_profile(&profile).unwrap();
        let state = AppState {
            database,
            secrets: Arc::new(WindowsCredentialStore),
            http: Arc::new(RestrictedHttpClient::new().unwrap()),
            exiting: AtomicBool::new(false),
            generation_in_progress: AtomicBool::new(false),
            auto_hide: Default::default(),
            persistence_gate: Default::default(),
        };
        let cases = [
            ("d01", "ask"),
            ("d02", "ask"),
            ("d03", "ask"),
            ("d05", "ask"),
            ("t10", "ask"),
            ("n01", "ask"),
            ("n02", "ask"),
            ("n04", "ask"),
            ("d02", "card"),
            ("t10", "card"),
        ];
        let only = std::env::var("TELLWHY_RAG_TEST_ONLY").ok();
        let cases: Vec<_> = cases
            .into_iter()
            .filter(|(id, kind)| {
                only.as_ref()
                    .map_or(true, |selected| selected == &format!("{id}:{kind}"))
            })
            .collect();
        assert!(!cases.is_empty(), "unknown live acceptance case");
        let expected_count = cases.len();
        let expected_cards = cases.iter().filter(|(_, kind)| *kind == "card").count();
        let report_name = if only.is_some() {
            "live-acceptance-selected.json"
        } else {
            "live-acceptance.json"
        };
        let mut outcomes = vec![];
        for (id, kind) in cases {
            let item = source["records"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["case"]["id"] == id)
                .unwrap();
            let task = json!({"id":uuid::Uuid::new_v4().to_string(),"kb":"computer-organization","kind":kind,
                "state":"prepared","created_at":chrono::Utc::now().to_rfc3339(),"provider":profile.provider_id,
                "region":profile.region,"model":profile.model,"packet":item["packet"],"result":null,"usage":[],
                "prompt_version":crate::rag::PROMPT_VERSION,"error":null});
            state.database.rag_insert(&task).unwrap();
            let result =
                crate::rag_commands::generate_inner(task["id"].as_str().unwrap().into(), &state)
                    .await;
            // Source review: d05 retrieves a consistency requirement and write policies,
            // but repeating that requirement does not explain its cause. Treat withholding
            // this draft as the grounding regression outcome, not a successful answer.
            let expected = if id == "d05" {
                "withheld"
            } else if id.starts_with('n') {
                "insufficient"
            } else {
                "answered"
            };
            let outcome = match result {
                Ok(value) => {
                    let passed = if expected == "withheld" {
                        (value["state"] == "completed"
                            && value["result"]["status"] == "insufficient")
                            || (value["state"] == "failed"
                                && value["error"]
                                    .as_str()
                                    .is_some_and(|e| e.starts_with("生成内容未通过教材依据检查")))
                    } else {
                        value["state"] == "completed" && value["result"]["status"] == expected
                    };
                    json!({"case":id,"kind":kind,"expected":expected,"passed":passed,"task":value})
                }
                Err(error) => json!({"case":id,"kind":kind,"passed":false,"error":error}),
            };
            println!("{} {} passed={}", id, kind, outcome["passed"]);
            outcomes.push(outcome);
            std::fs::write(report_dir.join(report_name),serde_json::to_vec_pretty(&json!({
                "provider":profile.provider_id,"region":profile.region,"model":profile.model,"results":outcomes})).unwrap()).unwrap();
            // Stop transport/config failures; do not repeat a potentially billable request.
            if outcomes.last().unwrap().get("error").is_some()
                || outcomes.last().unwrap()["task"]["error"]
                    .as_str()
                    .is_some_and(|e| e.starts_with("教材模型请求失败"))
            {
                break;
            }
        }
        assert_eq!(
            outcomes.len(),
            expected_count,
            "live acceptance stopped early; see report"
        );
        assert!(
            outcomes.iter().all(|o| o["passed"] == true),
            "live acceptance has failed cases; see report"
        );
        assert_eq!(
            state
                .database
                .rag_list("computer-organization", true, 0)
                .unwrap()["total"],
            expected_cards
        );
    });
}
